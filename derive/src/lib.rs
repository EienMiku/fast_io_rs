//! Derive an Encode implementation with static, allocation-free field dispatch.
//!
//! `#[encode("{field}")]` uses a field's `Encode` implementation; tuple fields
//! use their numeric index. `:x`, `:X`, `:b`, and `:o` select the corresponding
//! unsigned radix formatter, with `#` enabling its prefix (`0X` for `#X`).
//! `:?` uses `core::fmt::Debug` and does not require the field to implement
//! `Encode`. With the `float` feature, `.N`, `.Ne`, and `.NE` select fixed or
//! scientific notation with a literal precision in `0..=65535`. Width, dynamic
//! precision, implicit field indices, and other specifiers are rejected. Radix
//! and float fields preserve reserved output; Debug fields use streaming.
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use std::collections::BTreeSet;
use syn::{Data, DeriveInput, Fields, LitStr, Member, Type, parse_macro_input, parse_quote};

#[proc_macro_derive(Encode, attributes(encode))]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[derive(Default)]
struct Config {
    format: Option<LitStr>,
    transparent: bool,
    path: Option<syn::Path>,
}
fn config(attrs: &[syn::Attribute]) -> syn::Result<Config> {
    let mut cfg = Config::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("encode")) {
        if let Ok(format) = attr.parse_args::<LitStr>() {
            if cfg.format.replace(format).is_some() {
                return Err(syn::Error::new_spanned(attr, "duplicate encode format"));
            }
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("transparent") {
                cfg.transparent = true;
            } else if meta.path.is_ident("crate") {
                let text: LitStr = meta.value()?.parse()?;
                cfg.path = Some(text.parse()?);
            } else {
                return Err(
                    meta.error("expected a format string, transparent, or crate = \"path\"")
                );
            }
            Ok(())
        })?;
    }
    if cfg.transparent && cfg.format.is_some() {
        return Err(syn::Error::new(
            Span::call_site(),
            "transparent and a format string cannot be combined",
        ));
    }
    Ok(cfg)
}

struct Field {
    member: Member,
    ty: Type,
    label: String,
    skip: bool,
}
enum Part {
    Text(String),
    Field(usize, Format),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Format {
    Native,
    Debug,
    Radix {
        base: u8,
        uppercase: bool,
        prefix: bool,
    },
    Float {
        precision: u16,
        style: FloatStyle,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FloatStyle {
    Fixed,
    Scientific,
    UpperScientific,
}

impl Format {
    fn parse(specifier: &str, format: &LitStr) -> syn::Result<Self> {
        if let Some(precision) = specifier.strip_prefix('.') {
            let (precision, style) = if let Some(digits) = precision.strip_suffix('e') {
                (digits, FloatStyle::Scientific)
            } else if let Some(digits) = precision.strip_suffix('E') {
                (digits, FloatStyle::UpperScientific)
            } else {
                (precision, FloatStyle::Fixed)
            };
            let error = || {
                syn::Error::new_spanned(
                    format,
                    "float precision must be a decimal integer in 0..=65535, using .N, .Ne, or .NE",
                )
            };
            if precision.is_empty() || !precision.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(error());
            }
            let precision = precision.parse::<u16>().map_err(|_| error())?;
            return Ok(Self::Float { precision, style });
        }
        let (prefix, kind) = specifier
            .strip_prefix('#')
            .map_or((false, specifier), |kind| (true, kind));
        match (prefix, kind) {
            (false, "") => Ok(Self::Native),
            (false, "?") => Ok(Self::Debug),
            (_, "x" | "X" | "b" | "o") => Ok(Self::Radix {
                base: match kind {
                    "b" => 2,
                    "o" => 8,
                    _ => 16,
                },
                uppercase: kind == "X",
                prefix,
            }),
            _ => Err(syn::Error::new_spanned(
                format,
                format!(
                    "unsupported encode format specifier {specifier:?}; expected x, X, b, o (optionally #), ?, .N, .Ne, or .NE"
                ),
            )),
        }
    }

    fn expression(
        self,
        reference: proc_macro2::TokenStream,
        path: &syn::Path,
    ) -> proc_macro2::TokenStream {
        match self {
            Self::Native => reference,
            Self::Debug => quote!(#path::debug(#reference)),
            Self::Float { precision, style } => match style {
                FloatStyle::Fixed => quote!(#path::fixed(*(#reference), #precision)),
                FloatStyle::Scientific => quote!(#path::scientific(*(#reference), #precision)),
                FloatStyle::UpperScientific => {
                    quote!(#path::scientific(*(#reference), #precision).uppercase())
                }
            },
            Self::Radix {
                base,
                uppercase,
                prefix,
            } => {
                let formatter = match base {
                    2 => quote!(#path::bin),
                    8 => quote!(#path::oct),
                    _ => quote!(#path::hex),
                };
                let mut value = quote!(#formatter(*(#reference)));
                if uppercase {
                    value = quote!(#value.uppercase());
                }
                if prefix {
                    value = quote!(#value.prefixed());
                }
                value
            }
        }
    }
}

fn add_bounds(generics: &mut syn::Generics, parts: &[Part], fields: &[Field], path: &syn::Path) {
    let uses: BTreeSet<_> = parts
        .iter()
        .filter_map(|part| match part {
            Part::Field(index, format) => Some((*index, *format)),
            Part::Text(_) => None,
        })
        .collect();
    for (index, format) in uses {
        let ty = &fields[index].ty;
        let predicate = match format {
            Format::Native => parse_quote!(#ty: #path::Encode),
            Format::Debug => parse_quote!(#ty: ::core::fmt::Debug),
            Format::Float { .. } => parse_quote!(#ty: #path::Float),
            Format::Radix { .. } => {
                parse_quote!(#ty: #path::radix::Unsigned + ::core::marker::Copy)
            }
        };
        generics.make_where_clause().predicates.push(predicate);
    }
}

// Default layouts introduce separate name, punctuation and field-label pieces.
// Merge neighboring literals at expansion time so they occupy one fragment and
// require one write even when a field forces streaming output.
fn coalesce_parts(parts: Vec<Part>) -> Vec<Part> {
    let mut merged = Vec::with_capacity(parts.len());
    for part in parts {
        match part {
            Part::Text(text) if text.is_empty() => {}
            Part::Text(text) => {
                if let Some(Part::Text(previous)) = merged.last_mut() {
                    previous.push_str(&text);
                } else {
                    merged.push(Part::Text(text));
                }
            }
            field => merged.push(field),
        }
    }
    merged
}

fn parse_format(format: &LitStr, fields: &[Field]) -> syn::Result<Vec<Part>> {
    let value = format.value();
    let mut chars = value.chars().peekable();
    let mut text = String::new();
    let mut parts = Vec::new();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                if !text.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut text)));
                }
                let mut name = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    name.push(c);
                }
                if !closed {
                    return Err(syn::Error::new_spanned(
                        format,
                        "unclosed field placeholder",
                    ));
                }
                let (name, specifier) = name.split_once(':').unwrap_or((&name, ""));
                let mode = Format::parse(specifier, format)?;
                let index = fields.iter().position(|f| f.label == name && !f.skip)
                    .ok_or_else(|| syn::Error::new_spanned(format, format!("unknown or skipped field {{{name}}}; placeholders must name a field or tuple index")))?;
                parts.push(Part::Field(index, mode));
            }
            '}' => {
                return Err(syn::Error::new_spanned(
                    format,
                    "unmatched }; use }} for a literal brace",
                ));
            }
            c => text.push(c),
        }
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    Ok(parts)
}

fn fresh(base: &str, used: &mut BTreeSet<String>) -> syn::Ident {
    let mut name = base.to_owned();
    while !used.insert(name.clone()) {
        name.push('_');
    }
    format_ident!("{name}")
}

fn identifier_label(ident: &syn::Ident) -> String {
    let label = ident.to_string();
    label.strip_prefix("r#").unwrap_or(&label).to_owned()
}

fn used_names(input: &DeriveInput) -> BTreeSet<String> {
    input
        .generics
        .params
        .iter()
        .map(|param| match param {
            syn::GenericParam::Type(param) => identifier_label(&param.ident),
            syn::GenericParam::Const(param) => identifier_label(&param.ident),
            syn::GenericParam::Lifetime(param) => identifier_label(&param.lifetime.ident),
        })
        .collect()
}

fn collect_fields(spec: &Fields) -> syn::Result<Vec<Field>> {
    let mut fields = Vec::new();
    for (index, field) in spec.iter().enumerate() {
        let mut skip = false;
        for attr in field.attrs.iter().filter(|a| a.path().is_ident("encode")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    skip = true;
                    Ok(())
                } else {
                    Err(meta.error("expected skip on a field"))
                }
            })?;
        }
        fields.push(Field {
            member: field
                .ident
                .clone()
                .map(Member::Named)
                .unwrap_or_else(|| Member::Unnamed(index.into())),
            ty: field.ty.clone(),
            label: field
                .ident
                .as_ref()
                .map(identifier_label)
                .unwrap_or_else(|| index.to_string()),
            skip,
        });
    }
    Ok(fields)
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let cfg = config(&input.attrs)?;
    let path: syn::Path = cfg.path.clone().unwrap_or_else(|| parse_quote!(::fast_io));
    let name = &input.ident;
    if let Data::Enum(data) = &input.data {
        return expand_enum(&input, data, &cfg, &path);
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            name,
            "Encode cannot derive for a union",
        ));
    };
    let fields = collect_fields(&data.fields)?;
    let active: Vec<_> = fields
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.skip)
        .map(|(i, _)| i)
        .collect();
    let parts = if cfg.transparent {
        if active.len() != 1 {
            return Err(syn::Error::new_spanned(
                name,
                "transparent requires exactly one non-skipped field",
            ));
        }
        vec![Part::Field(active[0], Format::Native)]
    } else if let Some(format) = cfg.format {
        parse_format(&format, &fields)?
    } else {
        let mut parts = vec![Part::Text(identifier_label(name))];
        let (open, close) = match data.fields {
            Fields::Named(_) => (" { ", " }"),
            Fields::Unnamed(_) => ("(", ")"),
            Fields::Unit => ("", ""),
        };
        parts.push(Part::Text(open.into()));
        for (position, &index) in active.iter().enumerate() {
            if position != 0 {
                parts.push(Part::Text(", ".into()));
            }
            if matches!(data.fields, Fields::Named(_)) {
                parts.push(Part::Text(format!("{}: ", fields[index].label)));
            }
            parts.push(Part::Field(index, Format::Native));
        }
        parts.push(Part::Text(close.into()));
        parts
    };

    let parts = coalesce_parts(parts);
    let mut generics = input.generics.clone();
    add_bounds(&mut generics, &parts, &fields, &path);
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let mut used = used_names(&input);
    let sink = fresh("__FastIoSink", &mut used);
    let bytes = fresh("__FAST_IO_BYTES", &mut used);
    let pieces = fresh("__FAST_IO_PIECES", &mut used);
    let lifetime_name = fresh("__fast_io_value", &mut used);
    let lifetime = syn::Lifetime::new(&format!("'{lifetime_name}"), Span::call_site());
    let out_ident = fresh("output", &mut used);
    let batch_ident = fresh("batch", &mut used);

    let heads: Vec<_> = parts
        .iter()
        .filter_map(|part| match part {
            Part::Text(text) if text.is_empty() => None,
            Part::Text(text) => {
                let literal = LitStr::new(text, Span::call_site());
                Some((quote!(#literal), false))
            }
            Part::Field(index, format) => {
                let member = &fields[*index].member;
                let value = format.expression(quote!(&self.#member), &path);
                Some((value, *format != Format::Native))
            }
        })
        .collect();
    let arguments = argument_tree(&heads, &path);
    let streaming = parts.iter().filter_map(|part| match part {
        Part::Text(text) if text.is_empty() => None,
        Part::Text(text) => {
            let literal = syn::LitByteStr::new(text.as_bytes(), Span::call_site());
            Some(quote!(#path::Sink::write_all(#out_ident, #literal)?;))
        }
        Part::Field(index, format) => {
            let member = &fields[*index].member;
            let value = format.expression(quote!(&self.#member), &path);
            if *format == Format::Native {
                Some(quote!(#path::write(#out_ident, #value)?;))
            } else {
                Some(quote!(#path::write(#out_ident, &#value)?;))
            }
        }
    });
    let batch = parts.iter().filter_map(|part| match part {
        Part::Text(text) if text.is_empty() => None,
        Part::Text(text) => {
            let literal = LitStr::new(text, Span::call_site());
            Some(quote!(#batch_ident.push(#literal)?;))
        }
        Part::Field(index, format) => {
            let member = &fields[*index].member;
            let value = format.expression(quote!(&self.#member), &path);
            if *format == Format::Native {
                Some(quote!(#batch_ident.push(#value)?;))
            } else {
                // A temporary formatter cannot be borrowed for the batch's
                // entire lifetime. Its encoding is copied into scratch now.
                Some(quote!(#path::Encode::encode(&#value, #batch_ident)?;))
            }
        }
    });
    let borrowed = if cfg.transparent {
        let member = &fields[active[0]].member;
        quote! {
            fn borrowed_bytes(&self) -> impl #path::Presence<&[::core::primitive::u8]> {
                #path::Encode::borrowed_bytes(&self.#member)
            }
        }
    } else {
        quote!()
    };

    Ok(quote! {
        impl #impl_generics #path::Encode for #name #type_generics #where_clause {
            fn encode<#sink: #path::Sink + ?::core::marker::Sized>(&self, #out_ident: &mut #sink)
                -> ::core::result::Result<(), #sink::Error>
            {
                #(#streaming)*
                ::core::result::Result::Ok(())
            }
            #borrowed
            fn reserve_size(&self) -> impl #path::Presence<::core::primitive::usize> {
                #path::arguments::ReservedSize(#arguments)
            }
            fn encode_reserved(&self, #out_ident: &mut [::core::primitive::u8]) -> impl #path::Presence<::core::primitive::usize> {
                #path::arguments::ReservedWrite(#arguments, #out_ident)
            }
            fn encode_batch<#lifetime, #sink: #path::Sink + ?::core::marker::Sized, const #bytes: ::core::primitive::usize, const #pieces: ::core::primitive::usize>(
                &#lifetime self, #batch_ident: &mut #path::Batch<'_, #lifetime, #sink, #bytes, #pieces>
            ) -> impl #path::Presence<::core::result::Result<(), #sink::Error>> {
                #(#batch)*
                ::core::result::Result::Ok(())
            }
        }
    })
}

fn expand_enum(
    input: &DeriveInput,
    data: &syn::DataEnum,
    cfg: &Config,
    path: &syn::Path,
) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    if cfg.format.is_some() || cfg.transparent {
        return Err(syn::Error::new_spanned(
            name,
            "put enum formats or transparent on individual variants",
        ));
    }
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "an empty enum has no encodable values",
        ));
    }
    let mut generics = input.generics.clone();
    let mut stream_arms = Vec::new();
    let mut batch_arms = Vec::new();
    let mut size_arms = Vec::new();
    let mut reserved_arms = Vec::new();
    let mut used = used_names(input);
    let out_ident = fresh("output", &mut used);
    let batch_ident = fresh("batch", &mut used);
    for (variant_index, variant) in data.variants.iter().enumerate() {
        let variant_name = &variant.ident;
        let variant_cfg = config(&variant.attrs)?;
        if variant_cfg.path.is_some() {
            return Err(syn::Error::new_spanned(
                variant_name,
                "crate path belongs on the enum",
            ));
        }
        let fields = collect_fields(&variant.fields)?;
        let active: Vec<_> = fields
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.skip)
            .map(|(i, _)| i)
            .collect();
        let parts = if variant_cfg.transparent {
            if active.len() != 1 {
                return Err(syn::Error::new_spanned(
                    variant_name,
                    "transparent requires exactly one non-skipped field",
                ));
            }
            vec![Part::Field(active[0], Format::Native)]
        } else if let Some(format) = variant_cfg.format {
            parse_format(&format, &fields)?
        } else {
            let mut parts = vec![Part::Text(format!(
                "{}::{}",
                identifier_label(name),
                identifier_label(variant_name)
            ))];
            let (open, close) = match variant.fields {
                Fields::Named(_) => (" { ", " }"),
                Fields::Unnamed(_) => ("(", ")"),
                Fields::Unit => ("", ""),
            };
            parts.push(Part::Text(open.into()));
            for (position, &index) in active.iter().enumerate() {
                if position != 0 {
                    parts.push(Part::Text(", ".into()));
                }
                if matches!(variant.fields, Fields::Named(_)) {
                    parts.push(Part::Text(format!("{}: ", fields[index].label)));
                }
                parts.push(Part::Field(index, Format::Native));
            }
            parts.push(Part::Text(close.into()));
            parts
        };
        let parts = coalesce_parts(parts);
        let used_fields: BTreeSet<_> = parts
            .iter()
            .filter_map(|p| match p {
                Part::Field(i, _) => Some(*i),
                _ => None,
            })
            .collect();
        add_bounds(&mut generics, &parts, &fields, path);
        let bindings: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if used_fields.contains(&i) {
                    let id = fresh(&format!("__fast_io_field_{i}"), &mut used);
                    quote!(#id)
                } else {
                    quote!(_)
                }
            })
            .collect();
        let pattern = match &variant.fields {
            Fields::Named(_) => {
                let members = fields.iter().map(|f| &f.member);
                quote!(Self::#variant_name { #(#members: #bindings),* })
            }
            Fields::Unnamed(_) => quote!(Self::#variant_name(#(#bindings),*)),
            Fields::Unit => quote!(Self::#variant_name),
        };
        let stream = parts.iter().filter_map(|part| match part {
            Part::Text(text) if text.is_empty() => None,
            Part::Text(text) => {
                let literal = syn::LitByteStr::new(text.as_bytes(), Span::call_site());
                Some(quote!(#path::Sink::write_all(#out_ident, #literal)?;))
            }
            Part::Field(index, format) => {
                let binding = &bindings[*index];
                let value = format.expression(binding.clone(), path);
                if *format == Format::Native {
                    Some(quote!(#path::write(#out_ident, #value)?;))
                } else {
                    Some(quote!(#path::write(#out_ident, &#value)?;))
                }
            }
        });
        let batch = parts.iter().filter_map(|part| match part {
            Part::Text(text) if text.is_empty() => None,
            Part::Text(text) => {
                let literal = LitStr::new(text, Span::call_site());
                Some(quote!(#batch_ident.push(#literal)?;))
            }
            Part::Field(index, format) => {
                let binding = &bindings[*index];
                let value = format.expression(binding.clone(), path);
                if *format == Format::Native {
                    Some(quote!(#batch_ident.push(#value)?;))
                } else {
                    Some(quote!(#path::Encode::encode(&#value, #batch_ident)?;))
                }
            }
        });
        // Construct direct field borrows. Taking &binding would borrow a local
        // pattern variable and prevent returning the deferred capability.
        let mut heads = Vec::new();
        for part in &parts {
            let head = match part {
                Part::Text(text) if text.is_empty() => continue,
                Part::Text(text) => {
                    let literal = LitStr::new(text, Span::call_site());
                    (quote!(#literal), false)
                }
                Part::Field(index, format) => (
                    format.expression(bindings[*index].clone(), path),
                    *format != Format::Native,
                ),
            };
            heads.push(head);
        }
        let arguments = argument_tree(&heads, path);
        let size = reserved_choice(
            variant_index,
            data.variants.len(),
            quote!(#path::arguments::ReservedSize(#arguments)),
            path,
        );
        let reserved = reserved_choice(
            variant_index,
            data.variants.len(),
            quote!(#path::arguments::ReservedWrite(#arguments, #out_ident)),
            path,
        );
        stream_arms.push(quote!(#pattern => { #(#stream)* ::core::result::Result::Ok(()) }));
        batch_arms.push(quote!(#pattern => { #(#batch)* ::core::result::Result::Ok(()) }));
        size_arms.push(quote!(#pattern => #size));
        reserved_arms.push(quote!(#pattern => #reserved));
    }
    let sink = fresh("__FastIoSink", &mut used);
    let bytes = fresh("__FAST_IO_BYTES", &mut used);
    let pieces = fresh("__FAST_IO_PIECES", &mut used);
    let lifetime_name = fresh("__fast_io_value", &mut used);
    let lifetime = syn::Lifetime::new(&format!("'{lifetime_name}"), Span::call_site());
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #path::Encode for #name #type_generics #where_clause {
            fn encode<#sink: #path::Sink + ?::core::marker::Sized>(&self, #out_ident: &mut #sink)
                -> ::core::result::Result<(), #sink::Error>
            {
                match self { #(#stream_arms),* }
            }
            fn reserve_size(&self) -> impl #path::Presence<::core::primitive::usize> {
                match self { #(#size_arms),* }
            }
            fn encode_reserved(&self, #out_ident: &mut [::core::primitive::u8]) -> impl #path::Presence<::core::primitive::usize> {
                match self { #(#reserved_arms),* }
            }
            fn encode_batch<#lifetime, #sink: #path::Sink + ?::core::marker::Sized, const #bytes: ::core::primitive::usize, const #pieces: ::core::primitive::usize>(
                &#lifetime self, #batch_ident: &mut #path::Batch<'_, #lifetime, #sink, #bytes, #pieces>
            ) -> impl #path::Presence<::core::result::Result<(), #sink::Error>> {
                match self { #(#batch_arms),* }
            }
        }
    })
}

// A balanced sum type unifies different variants' opaque return types while
// deriving the overall Presence constant from all alternatives.
fn reserved_choice(
    index: usize,
    count: usize,
    value: proc_macro2::TokenStream,
    path: &syn::Path,
) -> proc_macro2::TokenStream {
    if count == 1 {
        return value;
    }
    let split = count.div_ceil(2);
    if index < split {
        let inner = reserved_choice(index, split, value, path);
        quote!(#path::arguments::ReservedChoice::Left(#inner))
    } else {
        let inner = reserved_choice(index - split, count - split, value, path);
        quote!(#path::arguments::ReservedChoice::Right(#inner))
    }
}

// Build the formatting list directly, with logarithmic depth. Going through
// recursive macro invocations hit the default recursion limit on wide structs.
fn argument_tree(
    heads: &[(proc_macro2::TokenStream, bool)],
    path: &syn::Path,
) -> proc_macro2::TokenStream {
    match heads {
        [] => quote!(#path::arguments::End),
        [(head, true)] => quote!(#path::arguments::Owned(#head)),
        [(head, false)] => {
            quote!(#path::arguments::Cons { head: #head, tail: #path::arguments::End })
        }
        _ => {
            let split = heads.len().div_ceil(2);
            let left = argument_tree(&heads[..split], path);
            let right = argument_tree(&heads[split..], path);
            quote!(#path::arguments::Joined { left: #left, right: #right })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_specs_and_unknown_fields_are_rejected() {
        let input: DeriveInput = parse_quote! {
            struct Item { value: u32, #[encode(skip)] hidden: u32 }
        };
        let Data::Struct(data) = input.data else {
            unreachable!()
        };
        let fields = collect_fields(&data.fields).unwrap();
        for format in ["{value:08x}", "{value:#?}", "{value:z}"] {
            let error = match parse_format(&LitStr::new(format, Span::call_site()), &fields) {
                Err(error) => error,
                Ok(_) => panic!("accepted unsupported format {format}"),
            };
            assert!(
                error
                    .to_string()
                    .contains("unsupported encode format specifier")
            );
        }
        for format in ["{:x}", "{other:x}", "{hidden:#x}"] {
            let error = match parse_format(&LitStr::new(format, Span::call_site()), &fields) {
                Err(error) => error,
                Ok(_) => panic!("accepted unknown or skipped field {format}"),
            };
            assert!(error.to_string().contains("unknown or skipped field"));
        }
        for format in [
            "{value:.}",
            "{value:.e}",
            "{value:.*}",
            "{value:.-1}",
            "{value:.2f}",
            "{value:.65536E}",
        ] {
            let error = match parse_format(&LitStr::new(format, Span::call_site()), &fields) {
                Err(error) => error,
                Ok(_) => panic!("accepted invalid precision {format}"),
            };
            assert!(error.to_string().contains("float precision must be"));
        }
        for format in [
            "{value:.0}",
            "{value:.65535}",
            "{value:.65535e}",
            "{value:.65535E}",
        ] {
            assert!(parse_format(&LitStr::new(format, Span::call_site()), &fields).is_ok());
        }
    }
}
