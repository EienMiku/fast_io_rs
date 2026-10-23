//! Derive an Encode implementation with static, allocation-free field dispatch.
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
    Field(usize),
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
                let index = fields.iter().position(|f| f.label == name && !f.skip)
                    .ok_or_else(|| syn::Error::new_spanned(format, format!("unknown or skipped field {{{name}}}; format specifiers are not supported")))?;
                parts.push(Part::Field(index));
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
                .map(ToString::to_string)
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
        vec![Part::Field(active[0])]
    } else if let Some(format) = cfg.format {
        parse_format(&format, &fields)?
    } else {
        let mut parts = vec![Part::Text(name.to_string())];
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
            parts.push(Part::Field(index));
        }
        parts.push(Part::Text(close.into()));
        parts
    };

    let used_fields: BTreeSet<_> = parts
        .iter()
        .filter_map(|p| match p {
            Part::Field(i) => Some(*i),
            _ => None,
        })
        .collect();
    let mut generics = input.generics.clone();
    for &index in &used_fields {
        let ty = &fields[index].ty;
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: #path::Encode));
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let mut used: BTreeSet<_> = input
        .generics
        .params
        .iter()
        .map(|p| match p {
            syn::GenericParam::Type(p) => p.ident.to_string(),
            syn::GenericParam::Const(p) => p.ident.to_string(),
            syn::GenericParam::Lifetime(p) => p.lifetime.ident.to_string(),
        })
        .collect();
    let sink = fresh("__FastIoSink", &mut used);
    let bytes = fresh("__FAST_IO_BYTES", &mut used);
    let pieces = fresh("__FAST_IO_PIECES", &mut used);
    let lifetime_name = fresh("__fast_io_value", &mut used);
    let lifetime = syn::Lifetime::new(&format!("'{lifetime_name}"), Span::call_site());

    let expressions: Vec<_> = parts
        .iter()
        .filter_map(|part| match part {
            Part::Text(text) if text.is_empty() => None,
            Part::Text(text) => {
                let literal = LitStr::new(text, Span::call_site());
                Some(quote!(#literal))
            }
            Part::Field(index) => {
                let member = &fields[*index].member;
                Some(quote!(self.#member))
            }
        })
        .collect();
    let streaming = parts.iter().filter_map(|part| match part {
        Part::Text(text) if text.is_empty() => None,
        Part::Text(text) => {
            let literal = syn::LitByteStr::new(text.as_bytes(), Span::call_site());
            Some(quote!(#path::Sink::write_all(output, #literal)?;))
        }
        Part::Field(index) => {
            let member = &fields[*index].member;
            Some(quote!(#path::write(output, &self.#member)?;))
        }
    });
    let borrowed = if cfg.transparent {
        let member = &fields[active[0]].member;
        quote! {
            fn borrowed_bytes(&self) -> impl #path::Presence<&[u8]> {
                #path::Encode::borrowed_bytes(&self.#member)
            }
        }
    } else {
        quote!()
    };

    Ok(quote! {
        impl #impl_generics #path::Encode for #name #type_generics #where_clause {
            fn encode<#sink: #path::Sink + ?Sized>(&self, output: &mut #sink)
                -> ::core::result::Result<(), #sink::Error>
            {
                #(#streaming)*
                ::core::result::Result::Ok(())
            }
            #borrowed
            fn reserve_size(&self) -> impl #path::Presence<usize> {
                #path::arguments::ReservedSize(#path::__fast_io_args!(#(#expressions),*))
            }
            fn encode_reserved(&self, output: &mut [u8]) -> impl #path::Presence<usize> {
                #path::arguments::ReservedWrite(#path::__fast_io_args!(#(#expressions),*), output)
            }
            fn encode_batch<#lifetime, #sink: #path::Sink + ?Sized, const #bytes: usize, const #pieces: usize>(
                &#lifetime self, batch: &mut #path::Batch<'_, #lifetime, #sink, #bytes, #pieces>
            ) -> impl #path::Presence<::core::result::Result<(), #sink::Error>> {
                #(batch.push(&#expressions)?;)*
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
            vec![Part::Field(active[0])]
        } else if let Some(format) = variant_cfg.format {
            parse_format(&format, &fields)?
        } else {
            let mut parts = vec![Part::Text(format!("{name}::{variant_name}"))];
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
                parts.push(Part::Field(index));
            }
            parts.push(Part::Text(close.into()));
            parts
        };
        let used_fields: BTreeSet<_> = parts
            .iter()
            .filter_map(|p| match p {
                Part::Field(i) => Some(*i),
                _ => None,
            })
            .collect();
        for &index in &used_fields {
            let ty = &fields[index].ty;
            generics
                .make_where_clause()
                .predicates
                .push(parse_quote!(#ty: #path::Encode));
        }
        let bindings: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if used_fields.contains(&i) {
                    let id = format_ident!("__fast_io_field_{i}");
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
                Some(quote!(#path::Sink::write_all(output, #literal)?;))
            }
            Part::Field(index) => {
                let binding = &bindings[*index];
                Some(quote!(#path::write(output, #binding)?;))
            }
        });
        let batch = parts.iter().filter_map(|part| match part {
            Part::Text(text) if text.is_empty() => None,
            Part::Text(text) => {
                let literal = LitStr::new(text, Span::call_site());
                Some(quote!(batch.push(#literal)?;))
            }
            Part::Field(index) => {
                let binding = &bindings[*index];
                Some(quote!(batch.push(#binding)?;))
            }
        });
        // Construct direct field borrows. Taking &binding would borrow a local
        // pattern variable and prevent returning the deferred capability.
        let mut arguments = quote!(#path::arguments::End);
        for part in parts.iter().rev() {
            let head = match part {
                Part::Text(text) if text.is_empty() => continue,
                Part::Text(text) => {
                    let literal = LitStr::new(text, Span::call_site());
                    quote!(#literal)
                }
                Part::Field(index) => bindings[*index].clone(),
            };
            arguments = quote!(#path::arguments::Cons { head: #head, tail: #arguments });
        }
        let size = reserved_choice(
            variant_index,
            data.variants.len(),
            quote!(#path::arguments::ReservedSize(#arguments)),
            path,
        );
        let reserved = reserved_choice(
            variant_index,
            data.variants.len(),
            quote!(#path::arguments::ReservedWrite(#arguments, output)),
            path,
        );
        stream_arms.push(quote!(#pattern => { #(#stream)* ::core::result::Result::Ok(()) }));
        batch_arms.push(quote!(#pattern => { #(#batch)* ::core::result::Result::Ok(()) }));
        size_arms.push(quote!(#pattern => #size));
        reserved_arms.push(quote!(#pattern => #reserved));
    }
    let mut used: BTreeSet<_> = input
        .generics
        .params
        .iter()
        .map(|p| match p {
            syn::GenericParam::Type(p) => p.ident.to_string(),
            syn::GenericParam::Const(p) => p.ident.to_string(),
            syn::GenericParam::Lifetime(p) => p.lifetime.ident.to_string(),
        })
        .collect();
    let sink = fresh("__FastIoSink", &mut used);
    let bytes = fresh("__FAST_IO_BYTES", &mut used);
    let pieces = fresh("__FAST_IO_PIECES", &mut used);
    let lifetime_name = fresh("__fast_io_value", &mut used);
    let lifetime = syn::Lifetime::new(&format!("'{lifetime_name}"), Span::call_site());
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #path::Encode for #name #type_generics #where_clause {
            fn encode<#sink: #path::Sink + ?Sized>(&self, output: &mut #sink)
                -> ::core::result::Result<(), #sink::Error>
            {
                match self { #(#stream_arms),* }
            }
            fn reserve_size(&self) -> impl #path::Presence<usize> {
                match self { #(#size_arms),* }
            }
            fn encode_reserved(&self, output: &mut [u8]) -> impl #path::Presence<usize> {
                match self { #(#reserved_arms),* }
            }
            fn encode_batch<#lifetime, #sink: #path::Sink + ?Sized, const #bytes: usize, const #pieces: usize>(
                &#lifetime self, batch: &mut #path::Batch<'_, #lifetime, #sink, #bytes, #pieces>
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
