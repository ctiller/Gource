// Golden values harness for C++ Gource file fade, touch, alpha, and physics calculations
#include <iostream>
#include <vector>
#include <cmath>
#include <iomanip>
#include <string>
#include <algorithm>

struct Vec2 {
    float x, y;
    Vec2() : x(0), y(0) {}
    Vec2(float x, float y) : x(x), y(y) {}
    Vec2 operator+(const Vec2& o) const { return Vec2(x + o.x, y + o.y); }
    Vec2 operator-(const Vec2& o) const { return Vec2(x - o.x, y - o.y); }
    Vec2 operator*(float s) const { return Vec2(x * s, y * s); }
    Vec2& operator+=(const Vec2& o) { x += o.x; y += o.y; return *this; }
    Vec2& operator-=(const Vec2& o) { x -= o.x; y -= o.y; return *this; }
    Vec2& operator*=(float s) { x *= s; y *= s; return *this; }
    float length() const { return std::sqrt(x * x + y * y); }
    float length2() const { return x * x + y * y; }
};

Vec2 normalise(const Vec2& v) {
    float len = v.length();
    if (len > 0.0f) return Vec2(v.x / len, v.y / len);
    return v;
}

struct Vec3 {
    float x, y, z;
    Vec3() : x(0), y(0), z(0) {}
    Vec3(float x, float y, float z) : x(x), y(y), z(z) {}
    Vec3 operator+(const Vec3& o) const { return Vec3(x + o.x, y + o.y, z + o.z); }
    Vec3 operator*(float s) const { return Vec3(x * s, y * s, z * s); }
};

// Simulate RFile logic step by step
void sim_file_steps() {
    float elapsed = 0.0f;
    float last_action = 0.0f;
    float fade_start = -1.0f;
    bool expired = false;
    bool removing = false;
    long long removed_timestamp = 0;
    Vec3 file_colour(0.2f, 0.4f, 0.6f);
    Vec3 touch_colour(1.0f, 1.0f, 1.0f);
    Vec2 pos(0.0f, 0.0f);
    Vec2 dest(1.0f, 0.0f);
    float distance = 100.0f;
    float speed = 5.0f;

    float dt = 0.1f;
    float file_idle_time = 3.0f;

    std::cout << "--- FILE_SIM ---" << std::endl;
    for (int step = 0; step < 50; ++step) {
        elapsed += dt;

        // File physics
        Vec2 dest_pos = dest * distance;
        Vec2 accel = dest_pos - pos;
        Vec2 accel2 = accel * speed * dt;
        if (accel2.length2() > accel.length2()) {
            accel2 = accel;
        }
        pos += accel2;

        // Idle fade
        if (fade_start < 0.0f && file_idle_time > 0.0f && (elapsed - last_action) > file_idle_time) {
            fade_start = elapsed;
        }

        bool just_expired = false;
        if (fade_start > 0.0f && !expired && (elapsed - fade_start) >= 1.0f) {
            expired = true;
            just_expired = true;
        }

        // Alpha
        float alpha = 1.0f;
        if (fade_start > 0.0f) {
            alpha = 1.0f - std::clamp(elapsed - fade_start, 0.0f, 1.0f);
        }

        // Colour
        Vec3 col = file_colour;
        float lc = elapsed - last_action;
        if (lc < 1.0f) {
            col = touch_colour * (1.0f - lc) + file_colour * lc;
        }

        if (step % 5 == 0 || just_expired) {
            std::cout << "FSTEP " << step << " t=" << elapsed 
                      << " pos=(" << pos.x << "," << pos.y << ")"
                      << " alpha=" << alpha 
                      << " col=(" << col.x << "," << col.y << "," << col.z << ")"
                      << " expired=" << (expired ? 1 : 0)
                      << " just_expired=" << (just_expired ? 1 : 0) << std::endl;
        }
    }
}

// Simulate User physics: friction, accel clamp
void sim_user_physics() {
    std::cout << "--- USER_PHYSICS ---" << std::endl;
    Vec2 pos(0.0f, 0.0f);
    Vec2 accel(300.0f, 400.0f); // length = 500
    float speed = 250.0f;
    float friction = 0.5f;
    float dt = 0.05f;

    for (int step = 0; step < 10; ++step) {
        if (accel.length2() > speed * speed) {
            accel = normalise(accel) * speed;
        }
        pos += accel * dt;
        accel = accel * std::max(0.0f, (1.0f - friction * dt));

        std::cout << "USTEP " << step << " pos=(" << pos.x << "," << pos.y << ")"
                  << " accel=(" << accel.x << "," << accel.y << ")" << std::endl;
    }
}

int main() {
    std::cout << std::fixed << std::setprecision(6);
    sim_file_steps();
    sim_user_physics();
    return 0;
}
