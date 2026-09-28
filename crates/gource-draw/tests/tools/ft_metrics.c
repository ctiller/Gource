/*
 * Tool to dump FreeType metrics for FreeSans.ttf exactly matching Gource's FXGlyphSet.
 *
 * Build command ($DEPS is a prefix holding the FreeType headers, e.g. an
 * extracted libfreetype-dev package; use /usr if it is installed system-wide):
 *   gcc -O2 -I$DEPS/usr/include/freetype2 \
 *       crates/gource-draw/tests/tools/ft_metrics.c /usr/lib/x86_64-linux-gnu/libfreetype.so.6 -lm -o crates/gource-draw/tests/tools/ft_metrics
 *
 * Run command:
 *   ./crates/gource-draw/tests/tools/ft_metrics data/fonts/FreeSans.ttf > crates/gource-draw/tests/data/freesans_metrics.txt
 */

#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_GLYPH_H

static const int SIZES[] = {10, 12, 14, 16, 18, 21, 24, 32, 42, 48};
#define NUM_SIZES (sizeof(SIZES) / sizeof(SIZES[0]))

// Characters to test:
// Printable ASCII (32 to 126 inclusive) plus:
// 'é' (0x00E9), 'ü' (0x00FC), 'ß' (0x00DF), '—' (em-dash 0x2014), 'ж' (0x0436), '€' (0x20AC)
static const unsigned int EXTRA_CHARS[] = {
    0x00E9, // é
    0x00FC, // ü
    0x00DF, // ß
    0x2014, // —
    0x0436, // ж
    0x20AC, // €
};
#define NUM_EXTRA_CHARS (sizeof(EXTRA_CHARS) / sizeof(EXTRA_CHARS[0]))

int main(int argc, char** argv) {
    const char* font_path = "data/fonts/FreeSans.ttf";
    if (argc > 1) {
        font_path = argv[1];
    }

    FT_Library freetype;
    if (FT_Init_FreeType(&freetype)) {
        fprintf(stderr, "Failed to initialize FreeType\n");
        return 1;
    }

    for (size_t s = 0; s < NUM_SIZES; ++s) {
        int size = SIZES[s];
        FT_Face ft_face;
        if (FT_New_Face(freetype, font_path, 0, &ft_face)) {
            fprintf(stderr, "Failed to load font face from %s\n", font_path);
            return 1;
        }

        int dpi = 72;
        int ft_font_size = 64 * size;
        FT_Set_Char_Size(ft_face, ft_font_size, ft_font_size, dpi, dpi);

        double em_size = 1.0 * ft_face->units_per_EM;
        double unit_scale_y = ft_face->size->metrics.y_ppem / em_size;

        float ascender = (float)(ft_face->ascender * unit_scale_y);
        float descender = (float)(ft_face->descender * unit_scale_y);
        float max_width = (float)(ft_face->size->metrics.max_advance / 64.0f);

        printf("SIZE %d ascender=%.6f descender=%.6f height=%.1f max_width=%.6f\n",
               size, ascender, descender, ceilf(ascender + descender), max_width);

        // Iterate over printable ASCII 32..126
        for (unsigned int chr = 32; chr <= 126; ++chr) {
            FT_UInt index = FT_Get_Char_Index(ft_face, chr);
            if (FT_Load_Glyph(ft_face, index, FT_LOAD_DEFAULT)) {
                fprintf(stderr, "Failed to load glyph %u\n", chr);
                continue;
            }
            FT_Glyph ftglyph;
            if (FT_Get_Glyph(ft_face->glyph, &ftglyph)) {
                fprintf(stderr, "Failed to get glyph %u\n", chr);
                continue;
            }
            int height = (int)ceil(ft_face->glyph->metrics.height / 64.0);
            FT_Glyph_To_Bitmap(&ftglyph, FT_RENDER_MODE_NORMAL, 0, 1);
            FT_BitmapGlyph glyph_bitmap = (FT_BitmapGlyph)ftglyph;

            int advance_x = ft_face->glyph->advance.x >> 6;
            int advance_y = ft_face->glyph->advance.y >> 6;
            int b_left = glyph_bitmap->left;
            int b_top = glyph_bitmap->top;
            int b_width = glyph_bitmap->bitmap.width;
            int b_rows = glyph_bitmap->bitmap.rows;

            printf("GLYPH size=%d char=%u adv_x=%d adv_y=%d h=%d left=%d top=%d w=%d rows=%d\n",
                   size, chr, advance_x, advance_y, height, b_left, b_top, b_width, b_rows);

            FT_Done_Glyph(ftglyph);
        }

        // Extra chars
        for (size_t i = 0; i < NUM_EXTRA_CHARS; ++i) {
            unsigned int chr = EXTRA_CHARS[i];
            FT_UInt index = FT_Get_Char_Index(ft_face, chr);
            if (FT_Load_Glyph(ft_face, index, FT_LOAD_DEFAULT)) {
                fprintf(stderr, "Failed to load glyph %u\n", chr);
                continue;
            }
            FT_Glyph ftglyph;
            if (FT_Get_Glyph(ft_face->glyph, &ftglyph)) {
                fprintf(stderr, "Failed to get glyph %u\n", chr);
                continue;
            }
            int height = (int)ceil(ft_face->glyph->metrics.height / 64.0);
            FT_Glyph_To_Bitmap(&ftglyph, FT_RENDER_MODE_NORMAL, 0, 1);
            FT_BitmapGlyph glyph_bitmap = (FT_BitmapGlyph)ftglyph;

            int advance_x = ft_face->glyph->advance.x >> 6;
            int advance_y = ft_face->glyph->advance.y >> 6;
            int b_left = glyph_bitmap->left;
            int b_top = glyph_bitmap->top;
            int b_width = glyph_bitmap->bitmap.width;
            int b_rows = glyph_bitmap->bitmap.rows;

            printf("GLYPH size=%d char=%u adv_x=%d adv_y=%d h=%d left=%d top=%d w=%d rows=%d\n",
                   size, chr, advance_x, advance_y, height, b_left, b_top, b_width, b_rows);

            FT_Done_Glyph(ftglyph);
        }

        FT_Done_Face(ft_face);
    }

    FT_Done_FreeType(freetype);
    return 0;
}
