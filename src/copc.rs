//! COPC VLR.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use las::{Bounds, Vector, Vlr};
use std::hash::Hash;
use std::io::{Cursor, Read, Write};

/// COPC Info VLR data.
#[derive(Clone, Debug, Default)]
pub struct CopcInfo {
    /// Actual (unscaled) coordinates of center of octree
    pub center: Vector<f64>,
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
    // Must be 0
    //_reserved: [u64; 11],
}

impl CopcInfo {
    /// Reads COPC VLR data from a `Read`.
    pub(crate) fn read_from<R: Read>(mut read: R) -> crate::Result<Self> {
        Ok(CopcInfo {
            center: Vector {
                x: read.read_f64::<LittleEndian>()?,
                y: read.read_f64::<LittleEndian>()?,
                z: read.read_f64::<LittleEndian>()?,
            },
            halfsize: read.read_f64::<LittleEndian>()?,
            spacing: read.read_f64::<LittleEndian>()?,
            root_hier_offset: read.read_u64::<LittleEndian>()?,
            root_hier_size: read.read_u64::<LittleEndian>()?,
            gpstime_minimum: read.read_f64::<LittleEndian>()?,
            gpstime_maximum: read.read_f64::<LittleEndian>()?,
            //_reserved: [0; 11],
        })
    }

    /// Convert COPC VLR data to a Vlr, size of VLR is 160bytes + header
    pub(crate) fn into_vlr(self) -> crate::Result<Vlr> {
        let mut buffer = Cursor::new([0_u8; 160]);

        buffer.write_f64::<LittleEndian>(self.center.x)?;
        buffer.write_f64::<LittleEndian>(self.center.y)?;
        buffer.write_f64::<LittleEndian>(self.center.z)?;
        buffer.write_f64::<LittleEndian>(self.halfsize)?;
        buffer.write_f64::<LittleEndian>(self.spacing)?;
        buffer.write_u64::<LittleEndian>(self.root_hier_offset)?;
        buffer.write_u64::<LittleEndian>(self.root_hier_size)?;
        buffer.write_f64::<LittleEndian>(self.gpstime_minimum)?;
        buffer.write_f64::<LittleEndian>(self.gpstime_maximum)?;

        Ok(Vlr {
            user_id: "copc".to_string(),
            record_id: 1,
            description: "COPC info VLR".to_string(),
            data: Vec::from(buffer.into_inner()),
        })
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
    pub(crate) fn read_from<R: Read>(read: &mut R) -> crate::Result<Self> {
        Ok(VoxelKey {
            level: read.read_i32::<LittleEndian>()?,
            x: read.read_i32::<LittleEndian>()?,
            y: read.read_i32::<LittleEndian>()?,
            z: read.read_i32::<LittleEndian>()?,
        })
    }

    /// Writes VoxelKey to a `Write`.
    pub(crate) fn write_to<W: Write>(self, write: &mut W) -> crate::Result<()> {
        write.write_i32::<LittleEndian>(self.level)?;
        write.write_i32::<LittleEndian>(self.x)?;
        write.write_i32::<LittleEndian>(self.y)?;
        write.write_i32::<LittleEndian>(self.z)?;

        Ok(())
    }

    pub(crate) fn child(&self, dir: i32) -> VoxelKey {
        VoxelKey {
            level: self.level + 1,
            x: (self.x << 1) | (dir & 0x1),
            y: (self.y << 1) | ((dir >> 1) & 0x1),
            z: (self.z << 1) | ((dir >> 2) & 0x1),
        }
    }
    pub(crate) fn children(&self) -> Vec<VoxelKey> {
        (0..8).map(|i| self.child(i)).collect()
    }
    pub(crate) fn bounds(&self, root_bounds: &Bounds) -> Bounds {
        // In an octree every cell is a cube
        let side_size =
            (root_bounds.max.x - root_bounds.min.x) / 2_u32.pow(self.level as u32) as f64;

        Bounds {
            min: Vector {
                x: root_bounds.min.x + self.x as f64 * side_size,
                y: root_bounds.min.y + self.y as f64 * side_size,
                z: root_bounds.min.z + self.z as f64 * side_size,
            },
            max: Vector {
                x: root_bounds.min.x + (self.x + 1) as f64 * side_size,
                y: root_bounds.min.y + (self.y + 1) as f64 * side_size,
                z: root_bounds.min.z + (self.z + 1) as f64 * side_size,
            },
        }
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
    pub(crate) fn read_from<R: Read>(read: &mut R) -> crate::Result<Self> {
        Ok(Entry {
            key: VoxelKey::read_from(read)?,
            offset: read.read_u64::<LittleEndian>()?,
            byte_size: read.read_i32::<LittleEndian>()?,
            point_count: read.read_i32::<LittleEndian>()?,
        })
    }

    /// Writes a hierarchy entry to a `Write`
    pub(crate) fn write_to<W: Write>(self, write: &mut W) -> crate::Result<()> {
        self.key.write_to(write)?;
        write.write_u64::<LittleEndian>(self.offset)?;
        write.write_i32::<LittleEndian>(self.byte_size)?;
        write.write_i32::<LittleEndian>(self.point_count)?;

        Ok(())
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
    pub(crate) fn read_from<R: Read>(mut read: R, page_size: u64) -> crate::Result<Self> {
        let num_entries = page_size as usize / 32;
        let mut entries = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            let entry = Entry::read_from(&mut read)?;
            entries.push(entry);
        }
        Ok(HierarchyPage { entries })
    }

    /// Writes a hierarchy page to a `Write`
    ///
    /// This implementation of COPC writer writes all ept entries to a single page
    pub(crate) fn into_evlr(self) -> crate::Result<Vlr> {
        // page size in bytes is the number of entries times 32 bytes per entry
        let mut buffer = Cursor::new(vec![0_u8; self.entries.len() * 32]);

        for e in self.entries {
            e.write_to(&mut buffer)?;
        }

        Ok(Vlr {
            user_id: "copc".to_string(),
            record_id: 1000,
            description: "EPT Hierarchy".to_string(),
            data: buffer.into_inner(),
        })
    }

    /// The number of bytes the data in the evlr is
    pub fn byte_size(&self) -> u64 {
        // each entry is 32 bytes
        (self.entries.len() * 32) as u64
    }
}

/// Our 'custom' type to build an octree from COPC hierarchy page
#[derive(Clone, Debug)]
pub(crate) struct OctreeNode {
    /// Hierarchy entry
    pub entry: Entry,
    /// The bounds this node represents, in file's coordinate
    pub bounds: Bounds,
    /// Children of this node, since its an octree, there
    /// are at most 8 children
    pub children: Vec<OctreeNode>,
}

impl OctreeNode {
    pub fn new() -> Self {
        OctreeNode {
            entry: Entry::default(),
            bounds: Bounds {
                min: Vector::default(),
                max: Vector::default(),
            },
            children: Vec::with_capacity(8),
        }
    }

    pub fn is_full(&self, max_size: i32) -> bool {
        self.entry.point_count >= max_size
    }
}
