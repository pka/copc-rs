//! COPC file reader.

use crate::bounds::Bounds;
use crate::copc::{CopcInfo, Entry, HierarchyPage, OctreeNode, VoxelKey};
use crate::decompressor::LasZipDecompressor;
use crate::header::Header;
use crate::vlr::Vlr;
use las::{Transform, Vector};
use laz::LazVlr;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom};

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

impl<R: Read + Seek + Send> CopcReader<R> {
    /// Setup by reading LAS header and LasZip VRLs
    pub fn open(mut src: R) -> std::io::Result<Self> {
        let las_header = Header::read_from(&mut src).unwrap();
        let copc_vlr = Vlr::read_from(&mut src).unwrap();
        if copc_vlr.user_id().as_str() != "copc" || copc_vlr.record_id != 1 {
            panic!("format error");
        }
        let copc_info = CopcInfo::read_from(Cursor::new(copc_vlr.data))?;
        // dbg!(&copc_info);
        // dbg!(&las_header);

        let mut reader = CopcReader {
            src,
            las_header,
            copc_info,
            laszip_vlr: None,
            projection_vlr: None,
            page_entries: HashMap::new(),
        };

        for _i in 0..reader.las_header.number_of_variable_length_records - 1 {
            let vlr = Vlr::read_from(&mut reader.src).unwrap();
            // dbg!(&vlr);
            match (vlr.user_id().as_str(), vlr.record_id) {
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
        query_bounds: &Option<Bounds>,
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
        let root_bounds = Bounds::new(
            info.center_x - info.halfsize,
            info.center_y - info.halfsize,
            info.center_z - info.halfsize,
            info.center_x + info.halfsize,
            info.center_y + info.halfsize,
            info.center_z + info.halfsize,
        );

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

            if let Some(bounds) = query_bounds {
                if !current_node.bounds.intersects(&bounds) {
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

                for child_key in current_node.entry.key.childs() {
                    let mut child_node = OctreeNode::new();
                    child_node.entry.key = child_key;
                    current_node.childs.push(child_node.clone());
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

    /// Point iterator for selected level
    pub fn points(
        &mut self,
        levels: LodSelection,
        bounds: Option<Bounds>,
    ) -> laz::Result<PointIter<R>> {
        let nodes = self.load_octree_for_query(levels, &bounds)?;
        let total_points_left = nodes.iter().map(|n| n.entry.point_count as usize).sum();

        // if bounds is not None:
        //     bounds = bounds.ensure_3d(self.header.mins, self.header.maxs)

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

/// LasZip point iterator
pub struct PointIter<'a, R: Read + Seek + Send> {
    nodes: Vec<OctreeNode>,
    bounds: Option<Bounds>,
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
        if self.node_points_left == 0 {
            if let Some(node) = self.nodes.pop() {
                self.decompressor.source_seek(node.entry.offset).unwrap();
                self.node_points_left = node.entry.point_count as usize;
            }
        }
        self.decompressor
            .decompress_one(self.point.as_mut_slice())
            .unwrap();
        self.node_points_left -= 1;
        self.total_points_left -= 1;
        if let Some(_bounds) = &self.bounds {
            // MINS = np.round(
            //     (bounds.mins - self.header.offsets) / self.header.scales
            // ).astype(np.int32)
            // MAXS = np.round(
            //     (bounds.maxs - self.header.offsets) / self.header.scales
            // ).astype(np.int32)
            // x_keep = (MINS[0] <= points.X) & (points.X <= MAXS[0])
            // y_keep = (MINS[1] <= points.Y) & (points.Y <= MAXS[1])
            // z_keep = (MINS[2] <= points.Z) & (points.Z <= MAXS[2])

            // # using scaled coordinates
            // # x, y, z = np.array(points.x), np.array(points.y), np.array(points.z)
            // # x_keep = (bounds.mins[0] <= x) & (x <= bounds.maxs[0])
            // # y_keep = (bounds.mins[1] <= y) & (y <= bounds.maxs[1])
            // # z_keep = (bounds.mins[2] <= z) & (z <= bounds.maxs[2])

            // keep_mask = x_keep & y_keep & z_keep
            // points.array = points.array[keep_mask].copy()
        }
        let raw_point =
            las::raw::Point::read_from(self.point.as_slice(), &self.point_format).unwrap();
        let point = las::point::Point::new(raw_point, &self.transforms);

        Some(point)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.total_points_left, Some(self.total_points_left))
    }
}
