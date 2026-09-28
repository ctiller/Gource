#!/usr/bin/env bash
# Script to generate golden files from the reference C++ Gource binary.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
DATA_DIR="$CRATE_DIR/tests/data"
GOURCE_REF="${GOURCE_REF:?set GOURCE_REF to a C++ gource binary built from src/}"

mkdir -p "$DATA_DIR/invalid" "$DATA_DIR/save_config"

echo "=== Generating Invalid Invocation Goldens ==="

# List of invalid invocations: name and arguments
declare -A INVALID_CASES=(
    ["unknown_option"]="--not-an-option"
    ["unknown_option_dash"]="---invalid"
    ["crop_missing"]="--crop"
    ["crop_invalid"]="--crop invalid"
    ["crop_invalid_num"]="--crop 123"
    ["viewport_invalid"]="--viewport invalid"
    ["viewport_extra"]="--viewport 1280x720x30"
    ["viewport_zeros"]="--viewport 0x0"
    ["viewport_neg"]="-viewport -100x100"
    ["screen_missing"]="--screen"
    ["screen_neg"]="--screen -1"
    ["screen_not_num"]="--screen abc"
    ["window_pos_missing"]="--window-position"
    ["window_pos_invalid"]="--window-position 100"
    ["window_pos_not_num"]="--window-position 100xabc"
    ["framerate_missing"]="--output-framerate"
    ["framerate_zero"]="--output-framerate 0"
    ["framerate_neg"]="--output-framerate -10"
    ["bloom_intensity_missing"]="--bloom-intensity"
    ["bloom_intensity_zero"]="--bloom-intensity 0.0"
    ["bloom_intensity_neg"]="--bloom-intensity -0.5"
    ["bloom_multiplier_missing"]="--bloom-multiplier"
    ["bloom_multiplier_zero"]="--bloom-multiplier 0.0"
    ["bloom_multiplier_neg"]="--bloom-multiplier -1.0"
    ["elasticity_missing"]="--elasticity"
    ["elasticity_zero"]="--elasticity 0.0"
    ["elasticity_neg"]="--elasticity -0.5"
    ["seconds_per_day_missing"]="--seconds-per-day"
    ["seconds_per_day_zero"]="--seconds-per-day 0"
    ["seconds_per_day_neg"]="--seconds-per-day -5"
    ["auto_skip_missing"]="--auto-skip-seconds"
    ["auto_skip_neg"]="--auto-skip-seconds -1.0"
    ["stop_at_time_missing"]="--stop-at-time"
    ["stop_at_time_zero"]="--stop-at-time 0.0"
    ["stop_at_time_neg"]="--stop-at-time -10.0"
    ["max_user_speed_missing"]="--max-user-speed"
    ["max_user_speed_zero"]="--max-user-speed 0"
    ["max_user_speed_neg"]="--max-user-speed -100"
    ["user_friction_missing"]="--user-friction"
    ["user_friction_neg"]="--user-friction -0.1"
    ["time_scale_missing"]="--time-scale"
    ["time_scale_zero"]="--time-scale 0.0"
    ["time_scale_neg"]="--time-scale -2.0"
    ["loop_delay_missing"]="--loop-delay-seconds"
    ["loop_delay_neg"]="--loop-delay-seconds -1.0"
    ["max_files_missing"]="--max-files"
    ["max_files_neg"]="--max-files -10"
    ["font_scale_missing"]="--font-scale"
    ["font_scale_neg"]="--font-scale -0.5"
    ["font_size_missing"]="--font-size"
    ["font_size_zero"]="--font-size 0"
    ["font_size_neg"]="--font-size -5"
    ["file_font_size_missing"]="--file-font-size"
    ["file_font_size_zero"]="--file-font-size 0"
    ["file_font_size_neg"]="--file-font-size -8"
    ["dir_font_size_missing"]="--dir-font-size"
    ["dir_font_size_zero"]="--dir-font-size 0"
    ["dir_font_size_neg"]="--dir-font-size -12"
    ["user_font_size_missing"]="--user-font-size"
    ["user_font_size_zero"]="--user-font-size 0"
    ["user_font_size_neg"]="--user-font-size -15"
    ["caption_size_missing"]="--caption-size"
    ["caption_size_zero"]="--caption-size 0"
    ["caption_size_neg"]="--caption-size -10"
    ["caption_duration_missing"]="--caption-duration"
    ["caption_duration_neg"]="--caption-duration -5"
    ["filename_time_missing"]="--filename-time"
    ["filename_time_neg"]="--filename-time -1"
    ["padding_missing"]="--padding"
    ["padding_zero"]="--padding 0"
    ["padding_neg"]="--padding -1.0"
    ["padding_too_high"]="--padding 2.5"
    ["dir_name_depth_missing"]="--dir-name-depth"
    ["dir_name_depth_zero"]="--dir-name-depth 0"
    ["dir_name_depth_neg"]="--dir-name-depth -2"
    ["dir_name_position_missing"]="--dir-name-position"
    ["dir_name_position_too_low"]="--dir-name-position 0.05"
    ["dir_name_position_too_high"]="--dir-name-position 1.5"
    ["start_position_missing"]="--start-position"
    ["start_position_zero"]="--start-position 0.0"
    ["start_position_one"]="--start-position 1.0"
    ["start_position_invalid"]="--start-position invalid"
    ["stop_position_missing"]="--stop-position"
    ["stop_position_zero"]="--stop-position 0.0"
    ["stop_position_invalid"]="--stop-position invalid"
    ["file_idle_time_missing"]="--file-idle-time"
    ["file_idle_time_neg"]="--file-idle-time -10"
    ["file_idle_time_at_end_missing"]="--file-idle-time-at-end"
    ["file_idle_time_at_end_neg"]="--file-idle-time-at-end -5"
    ["max_file_lag_missing"]="--max-file-lag"
    ["max_file_lag_zero"]="--max-file-lag 0"
    ["user_scale_missing"]="--user-scale"
    ["user_scale_zero"]="--user-scale 0.0"
    ["user_scale_neg"]="--user-scale -1.0"
    ["camera_mode_missing"]="--camera-mode"
    ["camera_mode_invalid"]="--camera-mode invalid"
    ["background_colour_missing"]="--background-colour"
    ["background_colour_invalid"]="--background-colour notacolor"
    ["font_colour_missing"]="--font-colour"
    ["font_colour_invalid"]="--font-colour 12345"
    ["dir_colour_missing"]="--dir-colour"
    ["dir_colour_invalid"]="--dir-colour badcolor"
    ["highlight_colour_missing"]="--highlight-colour"
    ["highlight_colour_invalid"]="--highlight-colour badcolor"
    ["selection_colour_missing"]="--selection-colour"
    ["selection_colour_invalid"]="--selection-colour badcolor"
    ["caption_colour_missing"]="--caption-colour"
    ["caption_colour_invalid"]="--caption-colour badcolor"
    ["user_filter_missing"]="--user-filter"
    ["user_filter_invalid_regex"]="--user-filter [unclosed"
    ["user_show_filter_missing"]="--user-show-filter"
    ["user_show_filter_invalid_regex"]="--user-show-filter (unclosed"
    ["file_filter_missing"]="--file-filter"
    ["file_filter_invalid_regex"]="--file-filter [unclosed"
    ["file_show_filter_missing"]="--file-show-filter"
    ["file_show_filter_invalid_regex"]="--file-show-filter *invalid"
    ["git_branch_missing"]="--git-branch"
    ["git_branch_dash"]="--git-branch -invalid"
    ["git_branch_badchars"]="--git-branch bad*branch"
    ["date_format_missing"]="--date-format"
    ["highlight_user_missing"]="--highlight-user"
    ["follow_user_missing"]="--follow-user"
    ["load_config_missing"]="--load-config"
    ["save_config_missing"]="--save-config"
    ["output_custom_log_missing"]="--output-custom-log"
    ["nonexistent_path"]="/path/to/nonexistent/gource_test_repo_12345"
)

for name in $(echo "${!INVALID_CASES[@]}" | tr ' ' '\n' | sort); do
    args="${INVALID_CASES[$name]}"
    # Run gource-ref with timeout, capture stderr and exit status
    set +e
    output=$(timeout 5 $GOURCE_REF $args 2>&1)
    status=$?
    set -e

    # First line usually has "gource: <reason>"
    # Extract the error reason: remove "gource: " prefix if present
    first_line=$(echo "$output" | head -n 1)
    reason=$(echo "$first_line" | sed 's/^gource: //')

    echo "Case: $name -> status=$status, reason='$reason'"

    # Store full output, first line reason, args, and status
    cat > "$DATA_DIR/invalid/${name}.json" <<EOF
{
  "name": "$name",
  "args": "$args",
  "status": $status,
  "reason": $(python3 -c "import json, sys; print(json.dumps(sys.argv[1]))" "$reason"),
  "first_line": $(python3 -c "import json, sys; print(json.dumps(sys.argv[1]))" "$first_line")
}
EOF
done

echo "=== Generating Valid --save-config Goldens ==="

# Test cases for --save-config
TMP_CONF="/tmp/gource_golden_save.conf"

run_save_case() {
    local name="$1"
    shift
    rm -f "$TMP_CONF"
    set +e
    $GOURCE_REF "$@" --save-config "$TMP_CONF" > /dev/null 2>&1
    local status=$?
    set -e
    if [ $status -eq 0 ] && [ -f "$TMP_CONF" ]; then
        cp "$TMP_CONF" "$DATA_DIR/save_config/${name}.conf"
        # Store invocation args
        echo "$*" > "$DATA_DIR/save_config/${name}.args"
        echo "Saved: $name"
    else
        echo "FAILED save case: $name (status $status)"
    fi
    rm -f "$TMP_CONF"
}

run_save_case "default"
run_save_case "display_viewport_res" -1920x1080
run_save_case "display_fullscreen_viewport" --fullscreen -1280x720
run_save_case "display_windowed" --windowed --screen 1
run_save_case "display_frameless_transparent" --frameless --transparent
run_save_case "display_sampling_vsync" --multi-sampling --no-vsync --high-dpi
run_save_case "display_output_ppm" --output-ppm-stream output.ppm -r 60
run_save_case "display_window_position" --window-position 100x200
run_save_case "gource_speed_bloom" --bloom-multiplier 2.5 --bloom-intensity 1.2 --seconds-per-day 5.0
run_save_case "gource_elasticity_skip" --elasticity 0.5 --auto-skip-seconds 2.0 --stop-at-time 60.0
run_save_case "gource_max_user_friction" --max-user-speed 400.0 --user-friction 0.8 --time-scale 1.5
run_save_case "gource_fonts" --font-scale 1.2 --font-size 18 --file-font-size 14 --dir-font-size 16 --user-font-size 20
run_save_case "gource_hides" --hide-date --hide-users --hide-progress --hide-bloom --hide-mouse --hide-root
run_save_case "gource_highlights" --highlight-users --highlight-dirs --file-extensions --fixed-user-size
run_save_case "gource_colors" --background-colour FF00FF --font-colour 112233 --highlight-colour 445566 --dir-colour 778899
run_save_case "gource_filters" --user-filter "bot.*" --user-filter "ci.*" --file-filter ".*\.min\.js" --file-filter "vendor/.*"
run_save_case "gource_shows" --user-show-filter "alice" --file-show-filter "src/.*"
run_save_case "gource_multivalue" --highlight-user alice --highlight-user bob --follow-user charlie
touch "$DATA_DIR/captions.txt"
# Relative to the crate directory (the working directory of cargo tests), so
# the golden doesn't depend on where the repository is checked out.
(cd "$CRATE_DIR" && run_save_case "gource_captions" --caption-file tests/data/captions.txt --caption-size 24 --caption-duration 4.0 --caption-colour FFFF00 --caption-offset 10)
run_save_case "gource_dir_settings" --dir-name-depth 3 --dir-name-position 0.75 --padding 1.2
run_save_case "gource_dates_positions" --start-date 2020-01-01 --stop-date 2020-12-31 --start-position 0.25 --stop-position 0.75
run_save_case "gource_misc" --max-files 500 --camera-mode overview --date-format "%d/%m/%y" --git-branch main
run_save_case "aliases" -f -w -o out.ppm -r 30 -p 0.3 -a 1.0 -s 10.0 -t 30.0 -i 20 -e 0.6 -b 000000 -c 2.0

echo "=== Golden Generation Complete ==="
