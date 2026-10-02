//! File extension legend display model (`LegendVM`, [`LegendRowVM`]).

use gource_core::Vec3;

/// A single row in the file extension legend.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendRowVM {
    pub ext: String,
    pub colour: Vec3,
    pub count: i32,
}

/// Display model for the file extension legend (`FileKey`).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LegendVM {
    pub visible: bool,
    pub rows: Vec<LegendRowVM>,
}
