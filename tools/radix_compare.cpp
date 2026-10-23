// Independent memory benchmark; upstream remains an external, unmodified checkout.
// Paired with examples/radix_throughput.rs and tools/compare_radix.py.
// Base 8 is deliberately excluded: the pinned upstream instantiates
// char_literal_add<unsigned __int128>, rejected by the current MSVC STL's
// std::integral constraint. No upstream headers or traits are changed here.
#include <fast_io_freestanding.h>
#include <array>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <stdexcept>
#include <string>
#include <string_view>

#ifndef __SIZEOF_INT128__
#error This benchmark requires native unsigned __int128 support.
#endif
using integer = unsigned __int128;
constexpr std::size_t value_count = 1024;

auto make_values() {
    constexpr integer multiplier = (integer{0xda942042e4dd58b5ULL} << 64) | 0x9e3779b97f4a7c15ULL;
    std::array<integer, value_count> values{};
    for (std::size_t i{}; i != value_count; ++i) values[i] = integer{i} * multiplier;
    values[0] = 0;
    values[1] = 1;
    values[2] = UINT8_MAX;
    values[3] = UINT16_MAX;
    values[4] = UINT32_MAX;
    values[5] = UINT64_MAX;
    values[6] = ~integer{};
    return values;
}

inline integer const& opaque(integer const& value) {
    auto pointer = &value;
    asm volatile("" : "+r"(pointer) : : "memory");
    return *pointer;
}
inline void escape(char const* data, std::size_t size) {
    asm volatile("" : : "r"(data), "r"(size) : "memory");
}

template<std::size_t Base>
void encode(fast_io::obuffer_view& output, integer value) {
    fast_io::operations::print_freestanding<false>(output, fast_io::mnp::base<Base>(value));
}

template<std::size_t Base>
void verify(std::array<integer, value_count> const& values, char const* path) {
    std::unique_ptr<std::FILE, decltype(&std::fclose)> file{std::fopen(path, "wb"), &std::fclose};
    if (!file) throw std::runtime_error("cannot create verification file");
    std::array<char, 130> storage{};
    fast_io::obuffer_view output{storage};
    for (auto value : values) {
        output.clear();
        encode<Base>(output, value);
        if (std::fwrite(output.data(), 1, output.size(), file.get()) != output.size() ||
            std::fputc('\n', file.get()) == EOF)
            throw std::runtime_error("verification output failed");
    }
    if (std::fflush(file.get())) throw std::runtime_error("verification flush failed");
}

template<std::size_t Base>
auto measure(std::array<integer, value_count> const& values, std::size_t count) {
    std::array<char, 130> storage{};
    fast_io::obuffer_view output{storage};
    std::uint64_t bytes{};
    auto start = std::chrono::steady_clock::now();
    for (std::size_t i{}; i != count; ++i) {
        output.clear();
        encode<Base>(output, opaque(values[i % value_count]));
        escape(output.data(), output.size());
        bytes += output.size();
    }
    auto elapsed = std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::steady_clock::now() - start).count();
    return std::pair{elapsed, bytes};
}

template<std::size_t Base>
void run(std::string_view mode, char const* argument) {
    auto values = make_values();
    if (mode == "verify") return verify<Base>(values, argument);
    if (mode != "print") throw std::runtime_error("MODE must be print or verify");
    auto count = std::stoull(argument);
    if (!count) throw std::runtime_error("COUNT must be positive");
    measure<Base>(values, 10000);
    auto [elapsed, bytes] = measure<Base>(values, count);
    std::printf("{\"elapsed_ns\":%lld,\"units\":%llu,\"bytes\":%llu}\n",
        static_cast<long long>(elapsed), static_cast<unsigned long long>(count),
        static_cast<unsigned long long>(bytes));
}

int main(int argc, char** argv) {
    if (argc != 4) throw std::runtime_error("MODE BASE COUNT or verify BASE PATH");
    auto base = std::stoul(argv[2]);
    switch (base) {
        case 2: run<2>(argv[1], argv[3]); break;
        case 16: run<16>(argv[1], argv[3]); break;
        default: throw std::runtime_error("C++ comparison supports BASE 2 or 16");
    }
}
