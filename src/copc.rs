//! COPC VLR.

use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Read;

/// COPC Info VLR data.
#[derive(Clone, Copy, Default, Debug)]
pub struct CopcInfo {
    /// Actual (unscaled) X coordinate of center of octree
    pub center_x: f64,
    /// Actual (unscaled) Y coordinate of center of octree
    pub center_y: f64,
    /// Actual (unscaled) Z coordinate of center of octree
    pub center_z: f64,
    /// Perpendicular distance from the center to any side of the root node.
    pub halfsize: f64,
    /// Space between points at the root node.
    /// This value is halved at each octree level
    pub spacing: f64,
    /// File offset to the first hierarchy page
    pub root_hier_offset: u64,
    /// Size of the first hierarchy page in bytes
    pub root_hier_size: u64,
    /// Minimum of GPSTime
    pub gpstime_minimum: f64,
    /// Maximum of GPSTime
    pub gpstime_maximum: f64,
    /// Must be 0
    _reserved: [u64; 11],
}

impl CopcInfo {
    /// Reads VLR data from a `Read`.
    pub fn read_from<R: Read>(mut read: R) -> std::io::Result<Self> {
        let mut data = CopcInfo::default();
        data.center_x = read.read_f64::<LittleEndian>()?;
        data.center_y = read.read_f64::<LittleEndian>()?;
        data.center_z = read.read_f64::<LittleEndian>()?;
        data.halfsize = read.read_f64::<LittleEndian>()?;
        data.spacing = read.read_f64::<LittleEndian>()?;
        data.root_hier_offset = read.read_u64::<LittleEndian>()?;
        data.root_hier_size = read.read_u64::<LittleEndian>()?;
        data.gpstime_minimum = read.read_f64::<LittleEndian>()?;
        data.gpstime_maximum = read.read_f64::<LittleEndian>()?;
        Ok(data)
    }
}

/// EPT hierarchy key
#[derive(Clone, Copy, Default, Debug)]
pub struct VoxelKey {
    /// Level
    ///
    /// A value < 0 indicates an invalid VoxelKey
    pub level: i32,
    /// x
    pub x: i32,
    /// y
    pub y: i32,
    /// z
    pub z: i32,
}

impl VoxelKey {
    /// Reads VoxelKey from a `Read`.
    pub fn read_from<R: Read>(read: &mut R) -> std::io::Result<VoxelKey> {
        let mut data = VoxelKey::default();
        data.level = read.read_i32::<LittleEndian>()?;
        data.x = read.read_i32::<LittleEndian>()?;
        data.y = read.read_i32::<LittleEndian>()?;
        data.z = read.read_i32::<LittleEndian>()?;
        Ok(data)
    }
}

/// Hierarchy entry
///
/// An entry corresponds to a single key/value pair in an EPT hierarchy, but contains additional information to allow direct access and decoding of the corresponding point data.
#[derive(Clone, Copy, Default, Debug)]
pub struct Entry {
    /// EPT key of the data to which this entry corresponds
    pub key: VoxelKey,

    /// Absolute offset to the data chunk if the pointCount > 0.
    /// Absolute offset to a child hierarchy page if the pointCount is -1.
    /// 0 if the pointCount is 0.
    pub offset: u64,

    /// Size of the data chunk in bytes (compressed size) if the pointCount > 0.
    /// Size of the hierarchy page if the pointCount is -1.
    /// 0 if the pointCount is 0.
    pub byte_size: i32,

    /// If > 0, represents the number of points in the data chunk.
    /// If -1, indicates the information for this octree node is found in another hierarchy page.
    /// If 0, no point data exists for this key, though may exist for child entries.
    pub point_count: i32,
}

impl Entry {
    /// Reads hierarchy entry from a `Read`.
    pub fn read_from<R: Read>(read: &mut R) -> std::io::Result<Entry> {
        let mut data = Entry::default();
        data.key = VoxelKey::read_from(read)?;
        data.offset = read.read_u64::<LittleEndian>()?;
        data.byte_size = read.read_i32::<LittleEndian>()?;
        data.point_count = read.read_i32::<LittleEndian>()?;
        Ok(data)
    }
}

/// Hierarchy page
///
/// COPC stores hierarchy information to allow a reader to locate points that are in a particular octree node.
/// The hierarchy may be arranged in a tree of pages, but shall always consist of at least one hierarchy page.
#[derive(Clone, Debug)]
pub struct Page {
    /// Hierarchy page entries
    pub entries: Vec<Entry>,
}

impl Page {
    /// Reads hierarchy page from a `Read`.
    pub fn read_from<R: Read>(mut read: R, page_size: u64) -> std::io::Result<Page> {
        let num_entries = page_size as usize / 32;
        let mut entries = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            let entry = Entry::read_from(&mut read)?;
            entries.push(entry)
        }
        Ok(Page { entries })
    }
}
