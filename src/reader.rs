//! COPC file reader.

use crate::copc::{CopcInfo, Entry, HierarchyPage, OctreeNode, VoxelKey};
use crate::decompressor::LasZipDecompressor;
use las::raw::{Header, Vlr};
use las::{Bounds, Error, Result, Transform, Vector};
use laz::LazVlr;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

const COPC: [u8; 4] = [67, 79, 80, 67];

/// COPC file reader
pub struct CopcReader<R> {
    src: R,
    las_header: Header,
    copc_info: CopcInfo,
    laszip_vlr: Option<LazVlr>,
    projection_vlr: Option<Vlr>,
    /// Entries of loaded hierarchy pages
    page_entries: HashMap<VoxelKey, Entry>,
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
    /// requested resolution of point cloud
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

impl CopcReader<BufReader<File>> {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        File::open(path)
            .map_err(Error::from)
            .and_then(|file| CopcReader::open(BufReader::new(file)))
    }
}

impl<R: Read + Seek + Send> CopcReader<R> {
    /// Setup by reading LAS header and LasZip VRLs
    pub fn open(mut src: R) -> Result<Self> {
        let las_header = Header::read_from(&mut src).unwrap();
        let copc_vlr = Vlr::read_from(&mut src, false).unwrap();
        if user_id_as_trimmed_string(&copc_vlr.user_id) != "copc" || copc_vlr.record_id != 1 {
            return Err(Error::InvalidFileSignature(COPC));
        }
        let copc_info = CopcInfo::read_from(Cursor::new(copc_vlr.data))?;

        let mut reader = CopcReader {
            src,
            las_header,
            copc_info,
            laszip_vlr: None,
            projection_vlr: None,
            page_entries: HashMap::new(),
        };

        for _i in 0..reader.las_header.number_of_variable_length_records - 1 {
            let vlr = Vlr::read_from(&mut reader.src, false).unwrap();
            // dbg!(&vlr);
            match (
                user_id_as_trimmed_string(&vlr.user_id).as_str(),
                vlr.record_id,
            ) {
                ("laszip encoded", 22204) => {
                    reader.laszip_vlr = Some(LazVlr::read_from(vlr.data.as_slice()).unwrap())
                }
                // ("copc", 1000) => reader.hierarchy_vlr = Some(vlr),
                ("LASF_Projection", 2112) => reader.projection_vlr = Some(vlr),
                (user_id, record_id) => {
                    eprintln!("Ignoring VLR {user_id}/{record_id}")
                }
            }
        }

        Ok(reader)
    }

    /// LAS header
    pub fn header(&self) -> &Header {
        &self.las_header
    }

    /// COPC info VLR content
    pub fn copc_info(&self) -> &CopcInfo {
        &self.copc_info
    }

    fn load_page(&mut self, offset: u64, byte_size: u64) -> std::io::Result<()> {
        self.src.seek(SeekFrom::Start(offset))?;
        let mut page = HierarchyPage::read_from(&mut self.src, byte_size)?;
        while let Some(entry) = page.entries.pop() {
            self.page_entries.insert(entry.key.clone(), entry);
        }
        Ok(())
    }

    /// Loads the nodes of the COPC octree that
    /// satisfies the parameters `query_bounds` and `level_range`.
    ///
    /// It returns the nodes of the matching 'sub-octree'
    fn load_octree_for_query(
        &mut self,
        level_range: LodSelection,
        query_bounds: &BoundsSelection,
    ) -> std::io::Result<Vec<OctreeNode>> {
        let (level_min, level_max) = match level_range {
            LodSelection::All => (0, i32::MAX),
            LodSelection::Resolution(resolution) => {
                let level_max =
                    1.max((self.copc_info.spacing / resolution).log2().ceil() as i32 + 1);
                (0, level_max)
            }
            LodSelection::Level(level) => (level, level + 1),
            LodSelection::LevelMinMax(min, max) => (min, max),
        };

        let info = &self.copc_info;
        let root_bounds = Bounds {
            min: Vector {
                x: info.center_x - info.halfsize,
                y: info.center_y - info.halfsize,
                z: info.center_z - info.halfsize,
            },
            max: Vector {
                x: info.center_x + info.halfsize,
                y: info.center_y + info.halfsize,
                z: info.center_z + info.halfsize,
            },
        };

        let mut root_node = OctreeNode::new();
        root_node.entry.key.level = 0;

        if self.page_entries.is_empty() {
            // Read root hierarchy page
            self.load_page(
                self.copc_info.root_hier_offset,
                self.copc_info.root_hier_size,
            )?;
        }

        let mut satisfying_nodes = Vec::new();
        let mut nodes_to_load = vec![root_node];

        while let Some(mut current_node) = nodes_to_load.pop() {
            current_node.bounds = current_node.entry.key.bounds(&root_bounds);

            if let BoundsSelection::Within(bounds) = query_bounds {
                if !bound_intersect(&current_node.bounds, bounds) {
                    continue;
                }
            }

            if current_node.entry.key.level >= level_max {
                continue;
            }

            let entry = self.page_entries.get(&current_node.entry.key);
            if entry.is_none() {
                continue;
            }
            let entry = entry.unwrap();

            // get the info of the node
            if entry.point_count == -1 {
                self.load_page(entry.offset, entry.byte_size as u64)?;
                nodes_to_load.push(current_node.clone());
            } else if entry.point_count != 0 {
                current_node.entry.offset = entry.offset;
                current_node.entry.byte_size = entry.byte_size;
                current_node.entry.point_count = entry.point_count;

                for child_key in current_node.entry.key.children() {
                    let mut child_node = OctreeNode::new();
                    child_node.entry.key = child_key;
                    current_node.children.push(child_node.clone());
                    nodes_to_load.push(child_node);
                }
            }

            // min <= level < max
            if (level_min..level_max).contains(&current_node.entry.key.level) {
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
    ) -> laz::Result<PointIter<R>> {
        let nodes = self.load_octree_for_query(levels, &bounds)?;
        let total_points_left = nodes.iter().map(|n| n.entry.point_count as usize).sum();

        let transforms = Vector {
            x: Transform {
                scale: self.las_header.x_scale_factor,
                offset: self.las_header.x_offset,
            },
            y: Transform {
                scale: self.las_header.y_scale_factor,
                offset: self.las_header.y_offset,
            },
            z: Transform {
                scale: self.las_header.z_scale_factor,
                offset: self.las_header.z_offset,
            },
        };

        // Reverse transform to unscaled values
        let bounds = match bounds {
            BoundsSelection::All => None,
            BoundsSelection::Within(bounds) => {
                let min_x = transforms.x.inverse(bounds.min.x).unwrap();
                let min_y = transforms.y.inverse(bounds.min.y).unwrap();
                let min_z = transforms.z.inverse(bounds.min.z).unwrap();
                let max_x = transforms.x.inverse(bounds.max.x).unwrap();
                let max_y = transforms.y.inverse(bounds.max.y).unwrap();
                let max_z = transforms.z.inverse(bounds.max.z).unwrap();
                Some([min_x, min_y, min_z, max_x, max_y, max_z])
            }
        };

        let laz_vlr = self
            .laszip_vlr
            .as_ref()
            .expect("Expected a laszip VLR for laz file");
        let decompressor = LasZipDecompressor::new(&mut self.src, Some(0), laz_vlr)?;

        let point_format =
            las::point::Format::new(self.las_header.point_data_record_format).unwrap();
        let point_size = self.las_header.point_data_record_length as usize;
        let point = vec![0u8; point_size];

        Ok(PointIter {
            nodes,
            bounds,
            point_format,
            transforms,
            decompressor,
            point,
            node_points_left: 0,
            total_points_left,
        })
    }
}

fn bound_intersect(a: &Bounds, b: &Bounds) -> bool {
    if a.max.x < b.min.x {
        return false;
    }
    if a.max.y < b.min.y {
        return false;
    }
    if a.max.z < b.min.z {
        return false;
    }
    if a.min.x > b.max.x {
        return false;
    }
    if a.min.y > b.max.y {
        return false;
    }
    if a.min.z > b.max.z {
        return false;
    }
    true
}

fn user_id_as_trimmed_string(bytes: &[u8; 16]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(|c| c as u8 == 0)
        .to_string()
}

/// LasZip point iterator
pub struct PointIter<'a, R: Read + Seek + Send> {
    nodes: Vec<OctreeNode>,
    bounds: Option<[i32; 6]>,
    point_format: las::point::Format,
    transforms: Vector<Transform>,
    decompressor: LasZipDecompressor<'a, &'a mut R>,
    point: Vec<u8>,
    node_points_left: usize,
    total_points_left: usize,
}

impl<'a, R: Read + Seek + Send> Iterator for PointIter<'a, R> {
    type Item = las::point::Point;

    fn next(&mut self) -> Option<Self::Item> {
        if self.total_points_left == 0 {
            return None;
        }
        let mut in_bounds = false;
        while !in_bounds {
            while self.node_points_left == 0 {
                if let Some(node) = self.nodes.pop() {
                    self.decompressor.source_seek(node.entry.offset).unwrap();
                    self.node_points_left = node.entry.point_count as usize;
                } else {
                    return None;
                }
            }
            self.decompressor
                .decompress_one(self.point.as_mut_slice())
                .unwrap();
            let raw_point =
                las::raw::Point::read_from(self.point.as_slice(), &self.point_format).unwrap();
            self.node_points_left -= 1;
            self.total_points_left -= 1;
            if let Some(bounds) = &self.bounds {
                let x_keep = (bounds[0] <= raw_point.x) && (raw_point.x <= bounds[3]);
                let y_keep = (bounds[1] <= raw_point.y) && (raw_point.y <= bounds[4]);
                let z_keep = (bounds[2] <= raw_point.z) && (raw_point.z <= bounds[5]);
                in_bounds = x_keep && y_keep && z_keep;
            } else {
                in_bounds = true;
            }

            if in_bounds {
                let point = las::point::Point::new(raw_point, &self.transforms);
                return Some(point);
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.total_points_left, Some(self.total_points_left))
    }
}
