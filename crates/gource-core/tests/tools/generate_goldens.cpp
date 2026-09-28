// Build instructions (run from this directory). $DEPS is a prefix holding the
// C++ Gource build dependencies (e.g. extracted -dev packages); drop the
// $DEPS flags if the dependencies are installed system-wide.
// g++ -std=gnu++17 \
//   -I../../../../src \
//   -I../../../../src/core \
//   -I$DEPS/usr/include \
//   -I$DEPS/usr/include/SDL2 \
//   -I$DEPS/usr/include/x86_64-linux-gnu \
//   -I$DEPS/usr/include/freetype2 \
//   -L$DEPS/lib \
//   -L$DEPS/usr/lib/x86_64-linux-gnu \
//   -Wl,-rpath,$DEPS/lib \
//   -Wl,-rpath,$DEPS/usr/lib/x86_64-linux-gnu \
//   ../../../../src/core/stringhash.cpp \
//   ../../../../src/core/vectors.cpp \
//   generate_goldens.cpp -o generate_goldens

#include <iostream>
#include <fstream>
#include <vector>
#include <string>
#include <iomanip>
#include "stringhash.h"
#include "utf8/utf8.h"

int main() {
    // Generate stringhash goldens
    std::ofstream sh_out("../data/stringhash_golden.txt");
    std::vector<std::string> test_strings = {
        "",
        "a",
        "ab",
        "abc",
        "main.cpp",
        "src/core/bounds.h",
        "long_test_string_with_numbers_1234567890_and_special_symbols_!@#$%^&*()",
        "hello world",
        "Gource C++ vs Rust Port",
        "é",
        "русский",
        "中文"
    };

    std::vector<int> seeds = {0, 1, 31, 42, 100, -5, 2147483647};

    for (int seed : seeds) {
        gStringHashSeed = seed;
        for (const auto& s : test_strings) {
            int h = stringHash(s);
            vec2 v2 = vec2Hash(s);
            vec3 v3 = vec3Hash(s);
            vec3 col = colourHash(s);
            sh_out << seed << "\t" << s << "\t" << h << "\t"
                   << std::setprecision(8) << v2.x << " " << v2.y << "\t"
                   << v3.x << " " << v3.y << " " << v3.z << "\t"
                   << col.x << " " << col.y << " " << col.z << "\n";
        }
    }
    sh_out.close();

    // Generate utf8 goldens
    std::ofstream u8_out("../data/utf8_golden.txt");
    std::vector<std::vector<unsigned char>> utf8_cases = {
        {},
        {'h', 'e', 'l', 'l', 'o'},
        {0xC3, 0xA9}, // é
        {0xE2, 0x82, 0xAC}, // €
        {0xF0, 0x9F, 0x9A, 0x80}, // 🚀
        {0x80}, // lone continuation
        {0x80, 0x80},
        {0x80, 'a'},
        {0xC0, 'a'}, // invalid lead
        {0xC1, 'a'},
        {0xC0, 0x80, 'a'}, // overlong
        {0xC1, 0xBF, 'a'},
        {0xF5, 'a'}, // invalid lead
        {0xFF, 'a'},
        {0xC2}, // truncated 2-byte
        {0xE0, 0x80}, // truncated 3-byte
        {'a', 0xE2, 0x82}, // truncated 3-byte at end
        {0xE2, 0x82, 'a'}, // truncated 3-byte mid
        {'a', 0xF0, 0x90, 0x80}, // truncated 4-byte at end
        {0xF0, 0x90, 'a'}, // truncated 4-byte mid
        {0xE2, 0x28, 0xA1}, // invalid trail in 3-byte
        {0xF0, 0x28, 0x80, 0x80}, // invalid trail in 4-byte
        {0xED, 0xA0, 0x80}, // surrogate lead
        {0xED, 0xBF, 0xBF}, // surrogate trail
        {0xF4, 0x90, 0x80, 0x80}, // out of range code point
        {0xFF, 0xFE, 0xFD}
    };

    for (const auto& raw : utf8_cases) {
        std::string s(raw.begin(), raw.end());
        std::string out;
        utf8::replace_invalid(s.begin(), s.end(), std::back_inserter(out), '?');

        // Write hex of input
        for (size_t i = 0; i < raw.size(); ++i) {
            u8_out << std::hex << std::setw(2) << std::setfill('0') << (int)raw[i];
        }
        u8_out << "\t";
        // Write hex of output
        for (size_t i = 0; i < out.size(); ++i) {
            u8_out << std::hex << std::setw(2) << std::setfill('0') << (int)(unsigned char)out[i];
        }
        u8_out << "\n";
    }
    u8_out.close();

    std::cout << "Golden generation complete!" << std::endl;
    return 0;
}
