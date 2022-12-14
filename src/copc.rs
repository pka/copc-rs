//! COPC VLR.

use crate::bounds::Bounds;
use byteorder::{LittleEndian, ReadBytesExt};
use std::hash::Hash;
use std::io::Read;

/// COPC Info VLR data.
#[derive(Clone, Default, Debug)]
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
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
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

impl Default for VoxelKey {
    fn default() -> Self {
        VoxelKey {
            level: -1,
            x: 0,
            y: 0,
            z: 0,
        }
    }
}

impl VoxelKey {
    /// Reads VoxelKey from a `Read`.
    pub fn read_from<R: Read>(read: &mut R) -> std::io::Result<Self> {
        let mut data = VoxelKey::default();
        data.level = read.read_i32::<LittleEndian>()?;
        data.x = read.read_i32::<LittleEndian>()?;
        data.y = read.read_i32::<LittleEndian>()?;
        data.z = read.read_i32::<LittleEndian>()?;
        Ok(data)
    }
    pub fn child(&self, dir: i32) -> VoxelKey {
        let mut key = VoxelKey::default();
        key.level = self.level + 1;
        key.x = (self.x << 1) | (dir & 0x1);
        key.y = (self.y << 1) | ((dir >> 1) & 0x1);
        key.z = (self.z << 1) | ((dir >> 2) & 0x1);
        key
    }
    pub fn childs(&self) -> Vec<VoxelKey> {
        (0..8).map(|i| self.child(i)).collect()
    }
    pub fn bounds(&self, root_bounds: &Bounds) -> Bounds {
        // In an octree every cell is a cube
        let side_size =
            (root_bounds.max_x - root_bounds.min_x) / 2_u32.pow(self.level as u32) as f64;
        let min_x = root_bounds.min_x + self.x as f64 * side_size;
        let min_y = root_bounds.min_y + self.y as f64 * side_size;
        let min_z = root_bounds.min_z + self.z as f64 * side_size;
        let max_x = root_bounds.min_x + (self.x + 1) as f64 * side_size;
        let max_y = root_bounds.min_y + (self.y + 1) as f64 * side_size;
        let max_z = root_bounds.min_z + (self.z + 1) as f64 * side_size;
        Bounds::new(min_x, min_y, min_z, max_x, max_y, max_z)
    }
}

/// Hierarchy entry
///
/// An entry corresponds to a single key/value pair in an EPT hierarchy, but contains additional information to allow direct access and decoding of the corresponding point data.
#[derive(Clone, Default, Debug)]
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
    pub fn read_from<R: Read>(read: &mut R) -> std::io::Result<Self> {
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
pub struct HierarchyPage {
    /// Hierarchy page entries
    pub entries: Vec<Entry>,
}

impl HierarchyPage {
    /// Reads hierarchy page from a `Read`.
    pub fn read_from<R: Read>(mut read: R, page_size: u64) -> std::io::Result<Self> {
        let num_entries = page_size as usize / 32;
        let mut entries = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            let entry = Entry::read_from(&mut read)?;
            entries.push(entry);
        }
        Ok(HierarchyPage { entries })
    }
}

/// Our 'custom' type to build an octree from COPC hierarchy page
#[derive(Clone, Debug)]
pub(crate) struct OctreeNode {
    /// Hierarchy entry
    pub entry: Entry,
    /// The bounds this node represents, in file's coordinate
    pub bounds: Bounds,
    /// Childs of this node, since its an octree, there
    /// are at most 8 childs
    pub childs: Vec<OctreeNode>,
}

impl OctreeNode {
    pub fn new() -> Self {
        OctreeNode {
            entry: Entry::default(),
            bounds: Bounds::new(0., 0., 0., 0., 0., 0.),
            childs: Vec::new(),
        }
    }
}
