// Golden values harness for C++ Gource pure math & simulation functions
#include <iostream>
#include <vector>
#include <cmath>
#include <iomanip>
#include <string>

const float PI = 3.14159265358979323846f;

struct Vec2 {
    float x, y;
    Vec2() : x(0), y(0) {}
    Vec2(float x, float y) : x(x), y(y) {}
    Vec2 operator+(const Vec2& o) const { return Vec2(x + o.x, y + o.y); }
    Vec2 operator-(const Vec2& o) const { return Vec2(x - o.x, y - o.y); }
    Vec2 operator*(float s) const { return Vec2(x * s, y * s); }
    Vec2 operator/(float s) const { return Vec2(x / s, y / s); }
    float length() const { return std::sqrt(x * x + y * y); }
    float dot(const Vec2& o) const { return x * o.x + y * o.y; }
};

struct Vec3 {
    float x, y, z;
    Vec3() : x(0), y(0), z(0) {}
    Vec3(float x, float y, float z) : x(x), y(y), z(z) {}
    Vec3 operator+(const Vec3& o) const { return Vec3(x + o.x, y + o.y, z + o.z); }
    Vec3 operator*(float s) const { return Vec3(x * s, y * s, z * s); }
};

struct Vec4 {
    float x, y, z, w;
    Vec4() : x(0), y(0), z(0), w(0) {}
    Vec4(float x, float y, float z, float w) : x(x), y(y), z(z), w(w) {}
    Vec4 operator+(const Vec4& o) const { return Vec4(x + o.x, y + o.y, z + o.z, w + o.w); }
    Vec4 operator*(float s) const { return Vec4(x * s, y * s, z * s, w * s); }
    Vec4 operator/(float s) const { return Vec4(x / s, y / s, z / s, w / s); }
};

Vec2 normalise(const Vec2& v) {
    float len = v.length();
    if (len > 0.0f) return v / len;
    return v;
}

// 1. SplineEdge::update
void test_spline(Vec2 pos1, Vec4 col1, Vec2 pos2, Vec4 col2, Vec2 spos, float dir_name_pos) {
    Vec2 mid = (pos1 - pos2) * 0.5f;
    Vec2 to = pos1 - spos;
    float dp = std::min(1.0f, normalise(to).dot(normalise(mid)));
    float ang = std::acos(dp) / PI;
    int edge_detail = std::min(10, (int)(ang * 100.0));
    if (edge_detail < 1) edge_detail = 1;

    std::vector<Vec2> points;
    std::vector<Vec4> cols;
    for (int i = 0; i <= edge_detail; ++i) {
        float t = (float)i / edge_detail;
        float tt = 1.0f - t;
        Vec2 p0 = pos1 * t + spos * tt;
        Vec2 p1 = spos * t + pos2 * tt;
        Vec2 pt = p0 * t + p1 * tt;
        Vec4 coln = col1 * t + col2 * tt;
        points.push_back(pt);
        cols.push_back(coln);
    }
    const float pos = dir_name_pos;
    const float s_quota = 0.5f - std::abs(pos - 0.5f);
    const float p_quota = 1.0f - s_quota;
    Vec2 label_pos = pos1 * (p_quota * (1.0f - pos)) + pos2 * (p_quota * pos) + spos * s_quota;

    std::cout << "SPLINE " << edge_detail << " " << label_pos.x << " " << label_pos.y << std::endl;
    for (size_t i = 0; i < points.size(); ++i) {
        std::cout << "PT " << i << " " << points[i].x << " " << points[i].y 
                  << " " << cols[i].x << " " << cols[i].y << " " << cols[i].z << " " << cols[i].w << std::endl;
    }
}

// 2. RDirNode::calcFileDest
void test_file_dest(int max_files, int file_no) {
    float arc = 1.0f / (float)max_files;
    float frac = arc * 0.5f + arc * file_no;
    Vec2 dest = Vec2(std::sin(frac * PI * 2.0f), std::cos(frac * PI * 2.0f));
    std::cout << "FILE_DEST " << max_files << " " << file_no << " " << dest.x << " " << dest.y << std::endl;
}

// 3. RDirNode::calcRadius
void test_calc_radius(float file_diameter, int visible_count, float children_area_sum, float dir_padding) {
    float padded_file_radius = file_diameter * 0.5f;
    float file_area = padded_file_radius * padded_file_radius * PI;
    float total_file_area = file_area * visible_count;
    float dir_area = total_file_area + children_area_sum;
    float dir_radius = std::max(1.0f, (float)std::sqrt(dir_area)) * dir_padding;
    float parent_radius = std::max(1.0f, (float)std::sqrt(total_file_area) * dir_padding);
    std::cout << "RADIUS " << dir_area << " " << dir_radius << " " << parent_radius << std::endl;
}

// 4. Pawn::nameAlpha
void test_pawn_name_alpha(float nametime, float name_interval) {
    float done = nametime - name_interval;
    float alpha;
    if (done < 1.0f) {
        alpha = done;
    } else if (done < nametime - 1.0f) {
        alpha = 1.0f;
    } else {
        alpha = (nametime - done);
    }
    std::cout << "NAME_ALPHA " << nametime << " " << name_interval << " " << alpha << std::endl;
}

int main() {
    std::cout << std::fixed << std::setprecision(6);
    // Spline test 1
    test_spline(Vec2(0.0f, 100.0f), Vec4(1.0f, 0.0f, 0.0f, 1.0f),
                Vec2(100.0f, 0.0f), Vec4(0.0f, 1.0f, 0.0f, 1.0f),
                Vec2(10.0f, 10.0f), 0.5f);
    // Spline test 2
    test_spline(Vec2(50.0f, -20.0f), Vec4(0.2f, 0.4f, 0.6f, 0.8f),
                Vec2(-10.0f, 80.0f), Vec4(0.8f, 0.6f, 0.4f, 0.2f),
                Vec2(0.0f, 0.0f), 0.3f);

    // File dest
    test_file_dest(1, 0);
    test_file_dest(3, 0);
    test_file_dest(3, 1);
    test_file_dest(3, 2);
    test_file_dest(6, 4);

    // Radius
    test_calc_radius(8.0f, 0, 0.0f, 1.5f);
    test_calc_radius(8.0f, 5, 200.0f, 1.5f);
    test_calc_radius(12.0f, 25, 1000.0f, 2.0f);

    // Name alpha
    test_pawn_name_alpha(5.0f, 4.5f);
    test_pawn_name_alpha(5.0f, 3.0f);
    test_pawn_name_alpha(5.0f, 0.5f);

    return 0;
}
