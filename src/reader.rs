//! COPC file reader.

use crate::copc::{CopcInfo, Entry, HierarchyPage, OctreeNode, VoxelKey};
use crate::decompressor::CopcDecompressor;
use las::raw;
use las::{Bounds, Builder, Header, Transform, Vector, Vlr};
use laz::LazVlr;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

/// COPC file reader
pub struct CopcReader<R> {
    // the start position of the data of interest in the read, most often 0
    start: u64,
    // the read- and seekable data source, seeked to the beginning of the copc file data
    read: R,
    header: Header,
    copc_info: CopcInfo,
    laz_vlr: LazVlr,
    /// Entries of loaded hierarchy pages
    hierarchy_entries: HashMap<VoxelKey, Entry>,
}

impl CopcReader<BufReader<File>> {
    /// Read a COPC file from a path, wraps the file in a BufRead for you
    pub fn from_path<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        File::open(path)
            .map_err(crate::Error::from)
            .and_then(|file| CopcReader::new(BufReader::new(file)))
    }
}

impl<R: Read + Seek> CopcReader<R> {
    /// Setup by reading LAS header and LasZip VLRs
    pub fn new(mut read: R) -> crate::Result<Self> {
        // to be able to read a copc file not starting at the beginning of the read stream
        let start = read.stream_position()?;

        let raw_header = raw::Header::read_from(&mut read)?;

        // store useful parts of the raw header before its consumed by the builder
        let mut position = raw_header.header_size as u64;
        let number_of_variable_length_records = raw_header.number_of_variable_length_records;
        let offset_to_point_data = raw_header.offset_to_point_data as u64;
        let evlr = raw_header.evlr;

        // start building a header from a raw header
        let mut builder = Builder::new(raw_header)?;

        // add the vlrs to the builder
        for _ in 0..number_of_variable_length_records {
            let vlr = raw::Vlr::read_from(&mut read, false).map(Vlr::new)?;
            position += vlr.len(false) as u64;
            builder.vlrs.push(vlr);
        }

        // adjust read pointer position and add the padding if it exists
        match position.cmp(&offset_to_point_data) {
            Ordering::Less => {
                let _ = read
                    .by_ref()
                    .take(offset_to_point_data + start - position)
                    .read_to_end(&mut builder.vlr_padding)?;
            }
            Ordering::Equal => {} // pass
            Ordering::Greater => Err(las::Error::OffsetToPointDataTooSmall(
                offset_to_point_data as u32,
            ))?,
        }

        // add the evlrs to the builder
        if let Some(evlr) = evlr {
            let _ = read.seek(SeekFrom::Start(evlr.start_of_first_evlr + start))?;
            for _ in 0..evlr.number_of_evlrs {
                builder
                    .evlrs
                    .push(raw::Vlr::read_from(&mut read, true).map(Vlr::new)?);
            }
        }

        // build the header
        let header = builder.into_header()?;

        // check and store the relevant (e)vlrs
        let mut copc_info = None;
        let mut laszip_vlr = None;
        let mut ept_hierarchy = None;

        for vlr in header.all_vlrs() {
            match (vlr.user_id.to_lowercase().as_str(), vlr.record_id) {
                ("copc", 1) => {
                    copc_info = Some(CopcInfo::read_from(vlr.data.as_slice())?);
                }
                ("copc", 1000) => {
                    ept_hierarchy = Some(vlr);
                }
                ("laszip encoded", 22204) => {
                    laszip_vlr = Some(LazVlr::read_from(vlr.data.as_slice())?);
                }
                _ => (),
            }
        }

        let copc_info = copc_info.ok_or(crate::Error::CopcInfoVlrNotFound)?;

        // store all ept-hierarchy entries in a hashmap
        let hierarchy_entries = match ept_hierarchy {
            None => return Err(crate::Error::EptHierarchyVlrNotFound),
            Some(vlr) => {
                let mut hierarchy_entries = HashMap::new();

                let mut read_vlr = Cursor::new(vlr.data.as_slice());

                // read the root hierarchy page
                let mut page =
                    HierarchyPage::read_from(&mut read_vlr, copc_info.root_hier_size)?.entries;

                while let Some(entry) = page.pop() {
                    if entry.point_count == -1 {
                        // read a new hierarchy page
                        read.seek(SeekFrom::Start(entry.offset - copc_info.root_hier_offset))?;
                        page.extend(
                            HierarchyPage::read_from(&mut read, entry.byte_size as u64)?.entries,
                        );
                    } else {
                        hierarchy_entries.insert(entry.key.clone(), entry);
                    }
                }
                hierarchy_entries
            }
        };

        // set the read pointer to the start of the compressed data block
        let _ = read.seek(SeekFrom::Start(offset_to_point_data + start))?;
        Ok(CopcReader {
            start,
            read,
            header,
            copc_info,
            laz_vlr: laszip_vlr.ok_or(crate::Error::LasZipVlrNotFound)?,
            hierarchy_entries,
        })
    }

    /// LAS header
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// COPC info VLR content
    pub fn copc_info(&self) -> &CopcInfo {
        &self.copc_info
    }

    pub fn num_entries(&self) -> usize {
        self.hierarchy_entries.len()
    }

    /// Loads the nodes of the COPC octree that
    /// satisfies the parameters `query_bounds` and `level_range`.
    ///
    /// It returns the nodes of the matching 'sub-octree'
    fn load_octree_for_query(
        &mut self,
        level_range: LodSelection,
        query_bounds: &BoundsSelection,
    ) -> crate::Result<Vec<OctreeNode>> {
        let (level_min, level_max) = match level_range {
            LodSelection::All => (0, i32::MAX),
            LodSelection::Resolution(resolution) => {
                if !resolution.is_normal() || !resolution.is_sign_positive() {
                    return Err(crate::Error::InvalidResolution(resolution));
                }
                (
                    0,
                    1.max((self.copc_info.spacing / resolution).log2().ceil() as i32 + 1),
                )
            }
            LodSelection::Level(level) => (level, level + 1),
            LodSelection::LevelMinMax(min, max) => (min, max),
        };

        let root_bounds = Bounds {
            min: Vector {
                x: self.copc_info.center.x - self.copc_info.halfsize,
                y: self.copc_info.center.y - self.copc_info.halfsize,
                z: self.copc_info.center.z - self.copc_info.halfsize,
            },
            max: Vector {
                x: self.copc_info.center.x + self.copc_info.halfsize,
                y: self.copc_info.center.y + self.copc_info.halfsize,
                z: self.copc_info.center.z + self.copc_info.halfsize,
            },
        };

        let mut root_node = OctreeNode::new();
        root_node.entry.key.level = 0;

        let mut satisfying_nodes = Vec::new();
        let mut node_stack = vec![root_node];

        while let Some(mut current_node) = node_stack.pop() {
            // bottom of tree of interest reached
            if current_node.entry.key.level >= level_max {
                continue;
            }

            let entry = match self.hierarchy_entries.get(&current_node.entry.key) {
                None => continue, // no entries for this node
                Some(e) => e,
            };

            current_node.bounds = current_node.entry.key.bounds(&root_bounds);
            if let BoundsSelection::Within(bounds) = query_bounds {
                // this octree node does not overlap with the bounds of interest
                if !bounds_intersect(&current_node.bounds, bounds) {
                    continue;
                }
            }

            // the entry exists and intersects with our interests
            // push its children to the node stack
            for child_key in current_node.entry.key.children() {
                let mut child_node = OctreeNode::new();
                child_node.entry.key = child_key;
                current_node.children.push(child_node.clone());
                node_stack.push(child_node);
            }

            // this node has points and belongs to the LOD of interest
            if entry.point_count > 0
                && (level_min..level_max).contains(&current_node.entry.key.level)
            {
                current_node.entry = entry.clone();
                satisfying_nodes.push(current_node);
            }
        }

        // Sort nodes by decending offsets for sequential reading
        satisfying_nodes.sort_by(|a, b| b.entry.offset.partial_cmp(&a.entry.offset).unwrap());

        Ok(satisfying_nodes)
    }

    /// Point iterator for selected level and bounds
    pub fn points(
        &mut self,
        levels: LodSelection,
        bounds: BoundsSelection,
    ) -> crate::Result<PointIter<'_, R>> {
        let nodes = self.load_octree_for_query(levels, &bounds)?;
        let total_points_left = nodes.iter().map(|n| n.entry.point_count as usize).sum();

        let transforms = *self.header().transforms();

        // Reverse transform to unscaled values
        let raw_bounds = match bounds {
            BoundsSelection::All => None,
            BoundsSelection::Within(bounds) => Some(RawBounds {
                min: Vector {
                    x: transforms.x.inverse(bounds.min.x)?,
                    y: transforms.y.inverse(bounds.min.y)?,
                    z: transforms.z.inverse(bounds.min.z)?,
                },
                max: Vector {
                    x: transforms.x.inverse(bounds.max.x)?,
                    y: transforms.y.inverse(bounds.max.y)?,
                    z: transforms.z.inverse(bounds.max.z)?,
                },
            }),
        };

        self.read.seek(SeekFrom::Start(self.start))?;
        let decompressor = CopcDecompressor::new(&mut self.read, &self.laz_vlr)?;
        let point = vec![
            0u8;
            (self.header.point_format().len() + self.header.point_format().extra_bytes)
                as usize
        ];

        Ok(PointIter {
            nodes,
            bounds: raw_bounds,
            point_format: *self.header.point_format(),
            transforms,
            decompressor,
            point_buffer: point,
            node_points_left: 0,
            total_points_left,
        })
    }
}

struct RawBounds {
    min: Vector<i32>,
    max: Vector<i32>,
}

impl RawBounds {
    #[inline]
    fn contains_point(&self, p: &las::raw::Point) -> bool {
        !(p.x < self.min.x
            || p.y < self.min.y
            || p.z < self.min.z
            || p.x > self.max.x
            || p.y > self.max.y
            || p.z > self.max.z)
    }
}

#[inline]
fn bounds_intersect(a: &Bounds, b: &Bounds) -> bool {
    !(a.max.x < b.min.x
        || a.max.y < b.min.y
        || a.max.z < b.min.z
        || a.min.x > b.max.x
        || a.min.y > b.max.y
        || a.min.z > b.max.z)
}

/// Limits the octree levels to be queried in order to have
/// a point cloud with the requested resolution.
///
/// resolution: Limits the octree levels to be queried in order
/// to have a point cloud with the requested resolution.
///
/// - The unit is the one of the data.
/// - If absent, the resulting cloud will be at the
///   full resolution offered by the COPC source
///
/// level: The level of detail (LOD).
///
/// If absent, all LOD are going to be considered
pub enum LodSelection {
    /// Full resolution (all LODs)
    All,
    /// requested minimal resolution of point cloud
    /// given as space between points
    /// based on the spacing given in the copc info vlr
    /// defined as root-node side length / number of points in root node
    /// when traversing the octree levels the spacing of level i is copc_spacing*2^-i
    ///
    /// Tldr; higher value -> fewer points / cube unit
    Resolution(f64),
    /// only points that that are of the requested LOD will be returned.
    Level(i32),
    /// points for which the LOD is within the range will be returned.
    LevelMinMax(i32, i32),
}

/// Select points within bounds
pub enum BoundsSelection {
    /// No bounds filter.
    All,
    /// Select points within bounds.
    Within(Bounds),
}

/// LasZip point iterator
pub struct PointIter<'a, R: Read + Seek> {
    nodes: Vec<OctreeNode>,
    bounds: Option<RawBounds>,
    point_format: las::point::Format,
    transforms: Vector<Transform>,
    decompressor: CopcDecompressor<'a, &'a mut R>,
    point_buffer: Vec<u8>,
    node_points_left: usize,
    total_points_left: usize,
}

impl<R: Read + Seek> Iterator for PointIter<'_, R> {
    type Item = las::point::Point;

    fn next(&mut self) -> Option<Self::Item> {
        if self.total_points_left == 0 {
            return None;
        }
        let mut in_bounds;
        loop {
            while self.node_points_left == 0 {
                // get the next node with points
                if let Some(node) = self.nodes.pop() {
                    self.decompressor.source_seek(node.entry.offset).unwrap();
                    self.node_points_left = node.entry.point_count as usize;
                } else {
                    return None;
                }
            }
            self.decompressor
                .decompress_one(self.point_buffer.as_mut_slice())
                .unwrap();
            let raw_point =
                las::raw::Point::read_from(self.point_buffer.as_slice(), &self.point_format)
                    .unwrap();
            self.node_points_left -= 1;
            self.total_points_left -= 1;
            in_bounds = if let Some(bounds) = &self.bounds {
                bounds.contains_point(&raw_point)
            } else {
                true
            };

            if in_bounds {
                return Some(las::point::Point::new(raw_point, &self.transforms));
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.total_points_left, Some(self.total_points_left))
    }
}
