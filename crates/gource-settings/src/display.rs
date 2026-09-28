//! Display settings (port of `SDLAppSettings` fields in core/settings.cpp).

use crate::SettingsError;
use crate::conffile::{ConfFile, ConfSection};

#[derive(Debug, Clone, PartialEq)]
pub struct DisplaySettings {
    pub display_width: i32,
    pub display_height: i32,
    /// Window position (`--window-position XxY`), -1 = unset.
    pub window_x: i32,
    pub window_y: i32,
    /// `--screen N`, -1 = default.
    pub screen: i32,
    /// True if `-WxH` / `--viewport` was given.
    pub viewport_specified: bool,
    pub multisample: bool,
    pub fullscreen: bool,
    pub frameless: bool,
    pub transparent: bool,
    pub resizable: bool,
    pub vsync: bool,
    pub high_dpi: bool,
    /// `--output-ppm-stream FILE` (`-` = stdout), empty = not recording.
    pub output_ppm_filename: String,
    /// `--output-framerate` (25, 30 or 60).
    pub output_framerate: i32,
}

impl Default for DisplaySettings {
    /// `SDLAppSettings::setDisplayDefaults` (non-Apple values).
    fn default() -> Self {
        Self {
            display_width: 1024,
            display_height: 768,
            window_x: -1,
            window_y: -1,
            screen: -1,
            viewport_specified: false,
            multisample: false,
            fullscreen: false,
            frameless: false,
            transparent: false,
            resizable: true,
            vsync: true,
            high_dpi: false,
            output_ppm_filename: String::new(),
            output_framerate: 60,
        }
    }
}

impl DisplaySettings {
    /// `importDisplaySettings`: read the `[display]` section, validating
    /// values with the C++ error messages.
    pub fn import(conf: &ConfFile) -> Result<Self, SettingsError> {
        let mut settings = Self::default();

        let display_settings = match conf.section("display") {
            Some(sec) => sec,
            None => return Ok(settings),
        };

        settings.viewport_specified = false;

        if let Some(entry) = display_settings.entry("viewport") {
            let viewport = &entry.value;
            if let Some((width, height, no_resize)) = parse_viewport(viewport) {
                settings.display_width = width;
                settings.display_height = height;
                if no_resize {
                    settings.resizable = false;
                }
                settings.viewport_specified = true;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = display_settings.entry("window-position") {
            let window_pos = &entry.value;
            if let Some((x, y)) = parse_rectangle(window_pos) {
                settings.window_x = x;
                settings.window_y = y;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = display_settings.entry("screen") {
            settings.screen = entry.get_int();
            if settings.screen < 1 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if display_settings.get_bool("multi-sampling") {
            settings.multisample = true;
        }

        if display_settings.get_bool("fullscreen") {
            settings.fullscreen = true;
        }

        if display_settings.get_bool("windowed") {
            settings.fullscreen = false;
        }

        if display_settings.get_bool("frameless") && !settings.fullscreen {
            settings.frameless = true;
        }

        // default to use desktop resolution for fullscreen unless specified
        if settings.fullscreen && !settings.viewport_specified {
            settings.display_width = 0;
            settings.display_height = 0;
        }

        if display_settings.get_bool("transparent") {
            settings.transparent = true;
        }

        if display_settings.get_bool("no-vsync") {
            settings.vsync = false;
        }

        if display_settings.get_bool("high-dpi") {
            settings.high_dpi = true;
        }

        if let Some(entry) = display_settings.entry("output-ppm-stream") {
            if !entry.has_value() {
                return Err(
                    conf.entry_error(Some(entry), "specify ppm output file or '-' for stdout")
                );
            }
            settings.output_ppm_filename = entry.value.clone();
        }

        if let Some(entry) = display_settings.entry("output-framerate") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify framerate (25,30,60)"));
            }
            settings.output_framerate = entry.get_int();
            if settings.output_framerate != 25
                && settings.output_framerate != 30
                && settings.output_framerate != 60
            {
                return Err(conf.entry_error(Some(entry), "supported framerates are 25,30,60"));
            }
        }

        Ok(settings)
    }

    /// `exportDisplaySettings`: write these settings into the `[display]`
    /// section of `conf`.
    pub fn export(&self, conf: &mut ConfFile) {
        let mut section = ConfSection {
            name: "display".to_owned(),
            entries: Vec::new(),
            line: 0,
        };

        let no_resize_suffix = if self.resizable { "" } else { "!" };
        let viewport = format!(
            "{}x{}{}",
            self.display_width, self.display_height, no_resize_suffix
        );
        section.set_entry("viewport", &viewport);

        if self.fullscreen {
            section.set_entry("fullscreen", "true");
        } else {
            if self.frameless {
                section.set_entry("frameless", "true");
            }
            if self.window_x >= 0 && self.window_y >= 0 {
                section.set_entry(
                    "window-position",
                    &format!("{}x{}", self.window_x, self.window_y),
                );
            }
        }

        if self.screen > 0 {
            section.set_entry("screen", &self.screen.to_string());
        }

        if self.multisample {
            section.set_entry("multi-sampling", "true");
        }

        if !self.vsync {
            section.set_entry("no-vsync", "true");
        }

        if self.high_dpi {
            section.set_entry("high-dpi", "true");
        }

        // C++: conf.setSection(section) -> replaces first section with that name
        set_conf_section(conf, section);
    }
}

pub(crate) fn set_conf_section(conf: &mut ConfFile, section: ConfSection) {
    if let Some(sec) = conf.section_mut(&section.name) {
        *sec = section;
    } else {
        conf.sections.push(section);
    }
}

pub(crate) fn parse_rectangle(value: &str) -> Option<(i32, i32)> {
    // Regex: ^([0-9.]+)x([0-9.]+)$
    let parts: Vec<&str> = value.split('x').collect();
    if parts.len() != 2 {
        return None;
    }
    if !is_numeric_component(parts[0]) || !is_numeric_component(parts[1]) {
        return None;
    }
    let x = crate::conffile::parse_c_int(parts[0]);
    let y = crate::conffile::parse_c_int(parts[1]);
    Some((x, y))
}

pub(crate) fn parse_viewport(value: &str) -> Option<(i32, i32, bool)> {
    // Regex: ^([0-9.]+)x([0-9.]+)(!)?$
    let (body, no_resize) = if let Some(stripped) = value.strip_suffix('!') {
        (stripped, true)
    } else {
        (value, false)
    };
    let parts: Vec<&str> = body.split('x').collect();
    if parts.len() != 2 {
        return None;
    }
    if !is_numeric_component(parts[0]) || !is_numeric_component(parts[1]) {
        return None;
    }
    let width = crate::conffile::parse_c_int(parts[0]);
    let height = crate::conffile::parse_c_int(parts[1]);
    if width > 0 && height > 0 {
        Some((width, height, no_resize))
    } else {
        None
    }
}

fn is_numeric_component(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_defaults() {
        let d = DisplaySettings::default();
        assert_eq!(d.display_width, 1024);
        assert_eq!(d.display_height, 768);
        assert_eq!(d.window_x, -1);
        assert_eq!(d.window_y, -1);
        assert_eq!(d.screen, -1);
        assert!(!d.viewport_specified);
        assert!(!d.fullscreen);
        assert!(d.resizable);
        assert!(d.vsync);
        assert_eq!(d.output_framerate, 60);
    }

    #[test]
    fn test_viewport_parsing() {
        assert_eq!(parse_viewport("1920x1080"), Some((1920, 1080, false)));
        assert_eq!(parse_viewport("1920x1080!"), Some((1920, 1080, true)));
        assert_eq!(parse_viewport("0x0"), None);
        assert_eq!(parse_viewport("invalid"), None);
    }

    #[test]
    fn test_rectangle_parsing() {
        assert_eq!(parse_rectangle("100x200"), Some((100, 200)));
        assert_eq!(parse_rectangle("invalid"), None);
    }

    #[test]
    fn test_import_export_roundtrip() {
        let mut conf = ConfFile::new();
        conf.set_entry("display", "viewport", "800x600!");
        conf.set_entry("display", "window-position", "50x60");
        conf.set_entry("display", "screen", "2");
        conf.set_entry("display", "multi-sampling", "true");
        conf.set_entry("display", "no-vsync", "true");
        conf.set_entry("display", "high-dpi", "true");
        conf.set_entry("display", "output-ppm-stream", "out.ppm");
        conf.set_entry("display", "output-framerate", "30");

        let settings = DisplaySettings::import(&conf).unwrap();
        assert_eq!(settings.display_width, 800);
        assert_eq!(settings.display_height, 600);
        assert!(!settings.resizable);
        assert_eq!(settings.window_x, 50);
        assert_eq!(settings.window_y, 60);
        assert_eq!(settings.screen, 2);
        assert!(settings.multisample);
        assert!(!settings.vsync);
        assert!(settings.high_dpi);
        assert_eq!(settings.output_ppm_filename, "out.ppm");
        assert_eq!(settings.output_framerate, 30);

        let mut conf_out = ConfFile::new();
        settings.export(&mut conf_out);
        let sec = conf_out.section("display").unwrap();
        assert_eq!(sec.get_string("viewport"), "800x600!");
        assert_eq!(sec.get_string("window-position"), "50x60");
        assert_eq!(sec.get_int("screen"), 2);
        assert!(sec.get_bool("multi-sampling"));
        assert!(sec.get_bool("no-vsync"));
        assert!(sec.get_bool("high-dpi"));
    }

    #[test]
    fn test_import_errors() {
        let mut conf = ConfFile::parse("[display]\nviewport=invalid\n", "test.conf").unwrap();
        assert_eq!(
            DisplaySettings::import(&conf).unwrap_err().0,
            "test.conf, line 2: invalid 'viewport' value"
        );

        conf = ConfFile::parse("[display]\noutput-framerate=45\n", "test.conf").unwrap();
        assert_eq!(
            DisplaySettings::import(&conf).unwrap_err().0,
            "test.conf, line 2: supported framerates are 25,30,60"
        );

        conf = ConfFile::parse("[display]\nscreen=0\n", "test.conf").unwrap();
        assert_eq!(
            DisplaySettings::import(&conf).unwrap_err().0,
            "test.conf, line 2: invalid 'screen' value"
        );
    }
}
