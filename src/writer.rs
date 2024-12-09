//! COPC file writer.

use crate::compressor::CopcCompressor;
use crate::copc::{CopcInfo, Entry, HierarchyPage, OctreeNode, VoxelKey};

use las::{Builder, Header};

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Cursor, Seek, SeekFrom, Write};
use std::path::Path;

/// COPC file writer
pub struct CopcWriter<'a, W: 'a + Write + Seek> {
    start: u64,
    // point writer
    compressor: CopcCompressor<'a, W>,
    header: Header,
    // a page of the written full entries
    hierarchy: HierarchyPage,
    max_node_size: i32,
    copc_info: CopcInfo,
    root_node: OctreeNode,
    // a
    open_chunks: HashMap<VoxelKey, Cursor<Vec<u8>>>,
}

impl CopcWriter<'_, BufWriter<File>> {
    /// Creates a new COPC-writer for a path,
    /// creates a file at that path and wraps it in a BufWrite for you
    /// and passes it along to [new]
    ///
    /// see [new] for usage
    ///
    /// [new]: Self::new
    pub fn from_path<D: Iterator<Item = las::Result<las::Point>> + Clone, P: AsRef<Path>>(
        path: P,
        header: Header,
        data: D,
        max_size: i32,
    ) -> crate::Result<Self> {
        let copc_ext = Path::new(match path.as_ref().file_stem() {
            Some(copc) => copc,
            None => return Err(crate::Error::WrongCopcExtension),
        })
        .extension();

        match (copc_ext, path.as_ref().extension()) {
            (Some(copc), Some(laz)) => match (&copc.to_str(), &laz.to_str()) {
                (Some(copc_str), Some(laz_str)) => {
                    if &copc_str.to_lowercase() != "copc" || &laz_str.to_lowercase() != "laz" {
                        return Err(crate::Error::WrongCopcExtension);
                    }
                }
                _ => return Err(crate::Error::WrongCopcExtension),
            },
            _ => return Err(crate::Error::WrongCopcExtension),
        }

        File::create(path)
            .map_err(crate::Error::from)
            .and_then(|file| CopcWriter::new(BufWriter::new(file), header, data, max_size))
    }
}

impl<W: Write + Seek> CopcWriter<'_, W> {
    /// Create a COPC file writer and writes the provided [Iterator] over [las::Result]<las::Point>
    /// to anything that implements [std::io::Write] and [std::io::Seek]
    /// configured with the provided [las::Header]
    /// recommended to use [from_path] for writing to file
    ///
    /// If the `bounds` field in the `header` is [las::Bounds::default()], optimal bounds are calculated
    /// from the data at the cost of iterating through the entire iterator one extra time
    /// else the provided bounds are used at the risk of returning a [crate::Error] if a point in data is outside the bounds
    /// returns a [crate::Error] if any item in data results in an [las::Error]
    ///
    /// `max_size` is the maximal number of [las::Point]s an octree node can hold
    /// any max_size < 1 sets the max_size to [crate::MAX_NODE_SIZE_DEFAULT]
    ///
    /// [from_path]: Self::from_path
    pub fn new<D: Iterator<Item = las::Result<las::Point>> + Clone>(
        mut write: W,
        header: Header,
        data: D,
        max_size: i32,
    ) -> crate::Result<Self> {
        let start = write.stream_position()?;

        let max_node_size = if max_size < 1 {
            crate::MAX_NODE_SIZE_DEFAULT
        } else {
            max_size
        };

        if !header.point_format().is_compressed {
            Err(las::Error::InvalidPointFormat(*header.point_format()))?;
        }

        if header.version() != las::Version::new(1, 4) {
            return Err(crate::Error::WrongLasVersion(header.version()));
        }

        // store the vlrs contained in the header for forwarding
        let mut forward_vlrs = Vec::with_capacity(header.vlrs().len());
        for vlr in header.vlrs() {
            match (vlr.user_id.to_lowercase().as_str(), vlr.record_id) {
                // not forwarding these vlrs
                ("copc", 1 | 1000) => (),
                ("laszip encoded", 22204) => (),
                ("lasf_spec", 100..355 | 65535) => (), // wave form packet descriptors
                // forwarding all other vlrs
                _ => forward_vlrs.push(vlr.clone()),
            }
        }

        // store the evlrs contained in the header for forwarding
        let mut forward_evlrs = Vec::with_capacity(header.evlrs().len());
        for evlr in header.evlrs() {
            match (evlr.user_id.to_lowercase().as_str(), evlr.record_id) {
                // not forwarding these vlrs
                ("copc", 1 | 1000) => (),        // 1 should never be a evlr
                ("laszip encoded", 22204) => (), // should never be a evlr
                ("lasf_spec", 100..355 | 65535) => (), // waveform data packets
                // forwarding all other evlrs
                _ => forward_evlrs.push(evlr.clone()),
            }
        }

        let calc_bounds = header.bounds() == las::Bounds::default();

        let mut raw_head = header.into_raw()?;

        // mask off the two leftmost bits corresponding to compression of pdrf
        if !(6..=8).contains(&(raw_head.point_data_record_format & 0b111111)) {
            // must be 6, 7 or 8
            Err(las::Error::InvalidPointFormat(las::point::Format::new(
                raw_head.point_data_record_format,
            )?))?;
        }
        // a las 1.4 header must be 375 bytes
        if raw_head.header_size != raw_head.version.header_size() {
            return Err(crate::Error::HeaderNot375Bytes(raw_head.header_size));
        }

        // clear some fields
        raw_head.global_encoding &= 0b11001; // set internal and external waveform bits (bit 1 and 2) to zero, all bits 5..=15 must be zero
        raw_head.number_of_point_records = 0;
        raw_head.number_of_points_by_return = [0; 5];
        raw_head.large_file = None;
        raw_head.evlr = None;
        raw_head.padding = vec![];

        if calc_bounds {
            let bounds_data = data.clone();

            let mut bounds = las::Bounds::default();
            for point in bounds_data {
                match point {
                    Err(e) => Err(e)?,
                    Ok(p) => bounds.grow(&p),
                }
            }
            bounds.adapt(&Default::default())?;
            raw_head.max_x = bounds.max.x;
            raw_head.max_y = bounds.max.y;
            raw_head.max_z = bounds.max.z;
            raw_head.min_x = bounds.min.x;
            raw_head.min_y = bounds.min.y;
            raw_head.min_z = bounds.min.z;
        }

        let mut builder = Builder::new(raw_head)?;
        // add a blank COPC-vlr as the first vlr
        builder.vlrs.push(CopcInfo::default().into_vlr()?);

        builder.vlrs.extend(forward_vlrs);
        builder.evlrs.extend(forward_evlrs);
        // the EPT-hierarchy evlr is not yet added

        let mut header = builder.into_header()?;
        header.add_laz_vlr()?;

        // write the header and vlrs
        // this is just to reserve the space
        header.write_to(&mut write)?;

        let mut root_node = OctreeNode::new();
        root_node.bounds = header.bounds();
        root_node.entry.key.level = 0;
        root_node.entry.offset = write.stream_position()? - start;

        let mut copc_info = CopcInfo::default();
        copc_info.center = las::Vector {
            x: (root_node.bounds.min.x + root_node.bounds.max.x) / 2.,
            y: (root_node.bounds.min.y + root_node.bounds.max.y) / 2.,
            z: (root_node.bounds.min.z + root_node.bounds.max.z) / 2.,
        };
        copc_info.halfsize = (copc_info.center.x - root_node.bounds.min.x).max(
            (copc_info.center.y - root_node.bounds.min.y)
                .max(copc_info.center.z - root_node.bounds.min.z),
        );

        let mut copc_writer = CopcWriter {
            start,
            compressor: CopcCompressor::new(write, header.laz_vlr().unwrap())?,
            header,
            hierarchy: HierarchyPage { entries: vec![] },
            max_node_size,
            copc_info,
            root_node,
            open_chunks: HashMap::default(),
        };

        for r_point in data.into_iter() {
            match r_point {
                Err(e) => Err(e)?,
                Ok(p) => {
                    if !p.matches(copc_writer.header.point_format()) {
                        Err(las::Error::PointAttributesDoNotMatch(
                            *copc_writer.header.point_format(),
                        ))?;
                    }
                    if !bounds_contains_point(&copc_writer.root_node.bounds, &p) {
                        return Err(crate::Error::PointNotInBounds);
                    }

                    copc_writer.add_point(p)?;
                }
            }
        }
        if copc_writer.header.number_of_points() < 1 {
            return Err(crate::Error::EmptyIterator);
        }

        copc_writer.close()?;

        Ok(copc_writer)
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn hierarchy_entries(&self) -> &HierarchyPage {
        &self.hierarchy
    }

    pub fn copc_info(&self) -> &CopcInfo {
        &self.copc_info
    }

    fn close(&mut self) -> crate::Result<()> {
        // write the unclosed chunks
        for (key, chunk) in self.open_chunks.drain() {
            let inner = chunk.into_inner();
            if inner.is_empty() {
                continue;
            }
            let (chunk_table_entry, chunk_offset) = self.compressor.compress_chunk(inner)?;
            self.hierarchy.entries.push(Entry {
                key,
                offset: chunk_offset,
                byte_size: chunk_table_entry.byte_count as i32,
                point_count: chunk_table_entry.point_count as i32,
            })
        }

        self.compressor.done()?;

        let start_of_first_evlr = self.compressor.get_mut().stream_position()? - self.start;

        let raw_evlrs: Vec<las::Result<las::raw::Vlr>> = {
            self.header
                .evlrs()
                .iter()
                .map(|evlr| evlr.clone().into_raw(true))
                .collect()
        };

        // write copc-evlr
        self.hierarchy
            .clone()
            .into_evlr()?
            .into_raw(true)?
            .write_to(self.compressor.get_mut())?;
        // write the rest of the evlrs
        for raw_evlr in raw_evlrs {
            raw_evlr?.write_to(self.compressor.get_mut())?;
        }

        let _ = self
            .compressor
            .get_mut()
            .seek(SeekFrom::Start(self.start))?;
        self.header.clone().into_raw().and_then(|mut raw_header| {
            if let Some(mut e) = raw_header.evlr {
                e.start_of_first_evlr = start_of_first_evlr;
                e.number_of_evlrs += 1;
            } else {
                raw_header.evlr = Some(las::raw::header::Evlr {
                    start_of_first_evlr,
                    number_of_evlrs: 1,
                });
            }
            raw_header.write_to(self.compressor.get_mut())
        })?;

        // update the copc info vlr and write it
        self.copc_info.spacing =
            self.copc_info.halfsize * 2. / self.hierarchy.entries[0].point_count as f64;
        self.copc_info.root_hier_offset = start_of_first_evlr;
        self.copc_info.root_hier_size = self.hierarchy.byte_size();

        self.copc_info
            .clone()
            .into_vlr()?
            .into_raw(false)?
            .write_to(self.compressor.get_mut())?;

        let _ = self
            .compressor
            .get_mut()
            .seek(SeekFrom::Start(self.start))?;
        Ok(())
    }

    // find the first non-full octree-node that contains the point
    // and add it to the node
    // if the node now is full
    // add it to the hierarchy page and write to file
    fn add_point(&mut self, point: las::Point) -> crate::Result<()> {
        self.header.add_point(&point);

        let raw_point = point.clone().into_raw(self.header.transforms())?;

        let mut node_key = None;
        let mut write_chunk = false;

        let root_bounds = self.root_node.bounds;
        let mut nodes_to_check = vec![&mut self.root_node];
        while let Some(node) = nodes_to_check.pop() {
            if !bounds_contains_point(&node.bounds, &point) {
                continue;
            }
            if node.is_full(self.max_node_size) {
                if node.children.is_empty() {
                    // add children to the node
                    let child_keys = node.entry.key.children();
                    for key in child_keys {
                        let child_bounds = key.bounds(&root_bounds);
                        node.children.push(OctreeNode {
                            entry: Entry {
                                key,
                                offset: 0,
                                byte_size: 0,
                                point_count: 0,
                            },
                            bounds: child_bounds,
                            children: Vec::with_capacity(8),
                        })
                    }
                }
                for child in node.children.iter_mut() {
                    nodes_to_check.push(child);
                }
                continue;
            }
            node_key = Some(node.entry.key.clone());
            node.entry.point_count += 1;

            write_chunk = node.is_full(self.max_node_size);
        }
        if node_key.is_none() {
            return Err(crate::Error::PointNotAddedToAnyNode);
        }
        let node_key = node_key.unwrap();

        if !self.open_chunks.contains_key(&node_key) {
            let mut val = Cursor::new(vec![]);
            raw_point.write_to(&mut val, self.header.point_format())?;

            self.open_chunks.insert(node_key.clone(), val);
        } else {
            let buffer = self.open_chunks.get_mut(&node_key).unwrap();
            raw_point.write_to(buffer, self.header.point_format())?;
        }

        if write_chunk {
            let chunk = self.open_chunks.remove(&node_key).unwrap();
            let (chunk_table_entry, chunk_offset) =
                self.compressor.compress_chunk(chunk.into_inner())?;
            self.hierarchy.entries.push(Entry {
                key: node_key,
                offset: chunk_offset,
                byte_size: chunk_table_entry.byte_count as i32,
                point_count: chunk_table_entry.point_count as i32,
            })
        }
        Ok(())
    }
}

fn bounds_contains_point(b: &las::Bounds, p: &las::Point) -> bool {
    !(b.max.x < p.x
        || b.max.y < p.y
        || b.max.z < p.z
        || b.min.x > p.x
        || b.min.y > p.y
        || b.min.z > p.z)
}
