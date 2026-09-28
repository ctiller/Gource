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
//   ../../../../src/core/regex.cpp \
//   -lpcre2-8 \
//   generate_datetime_goldens.cpp -o generate_datetime_goldens

#include <iostream>
#include <fstream>
#include <vector>
#include <string>
#include <cstring>
#include <ctime>
#include "regex.h"

// Replicating SDLAppSettings::parseDateTime logic in C++ for UTC timezone
bool parseDateTimeUTC(const std::string& datetime, time_t& timestamp) {
    Regex timestamp_regex("^(\\d{4})-(\\d{2})-(\\d{2})(?:[T ](\\d{1,2}):(\\d{2})(?::(\\d{2}(?:\\.\\d+)?))?)?(Z| ?([+-])(\\d{1,2})(?::?(\\d{2}))?)?$");
    std::vector<std::string> results;
    if(!timestamp_regex.match(datetime, &results) || results.size() < 3) return false;

    struct tm timeinfo;
    memset(&timeinfo, 0, sizeof(timeinfo));
    timeinfo.tm_isdst = 0;
    timeinfo.tm_year = atoi(results[0].c_str()) - 1900;
    timeinfo.tm_mon  = atoi(results[1].c_str()) - 1;
    timeinfo.tm_mday = atoi(results[2].c_str());

    if(results.size() >= 5 && results[3] != "") {
        timeinfo.tm_hour = atoi(results[3].c_str());
        timeinfo.tm_min  = atoi(results[4].c_str());
        if(results.size() >= 6) {
            timeinfo.tm_sec  = atoi(results[5].c_str());
        }
    }

    if(results.size() == 7 && results[6] == "Z") {
        timestamp = timegm(&timeinfo);
    } else if(results.size() >= 9) {
        int tz_hour = atoi(results[8].c_str());
        int tz_min  = 0;
        if(results.size() >= 10) {
            tz_min = atoi(results[9].c_str());
        }
        int tz_offset = tz_hour * 3600 + tz_min * 60;
        if(results[7] == "-") {
            tz_offset = -tz_offset;
        }
        timestamp = timegm(&timeinfo);
        timestamp -= tz_offset;
    } else {
        timestamp = timegm(&timeinfo);
    }
    return true;
}

int main() {
    std::ofstream dt_out("../data/datetime_golden.txt");
    std::vector<std::string> test_dates = {
        "2010-01-02",
        "2010-01-02Z",
        "2010-01-02 03:04",
        "2010-01-02 03:04:05",
        "2010-01-01 03:04:05+12",
        "2010-01-01T03:04",
        "2010-01-01T03:04:05",
        "2010-01-01T03:04:05Z",
        "2010-01-01T03:04:05+12",
        "2010-01-01T00:04:05+5:30",
        "2010-01-01T03:04:05.6789",
        "2021-11-01",
        "2021-11-01 +13",
        "2021-11-01-08",
        "not a date"
    };

    for (const auto& s : test_dates) {
        time_t ts = 0;
        bool ok = parseDateTimeUTC(s, ts);
        dt_out << s << "\t" << (ok ? "1" : "0") << "\t" << (ok ? std::to_string(ts) : "0") << "\n";
    }
    dt_out.close();

    std::cout << "Datetime golden generation complete!" << std::endl;
    return 0;
}
