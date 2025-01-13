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
    is_closed: bool,
    start: u64,
    // point writer
    compressor: CopcCompressor<'a, W>,
    header: Header,
    // a page of the written entries
    hierarchy: HierarchyPage,
    max_node_size: i32,
    copc_info: CopcInfo,
    root_node: OctreeNode,
    // a hashmap to store chunks that are not full yet
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
    pub fn from_path<P: AsRef<Path>>(
        path: P,
        header: Header,
        min_size: i32,
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
            .and_then(|file| CopcWriter::new(BufWriter::new(file), header, min_size, max_size))
    }
}

impl<W: Write + Seek> CopcWriter<'_, W> {
    /// Create a COPC file writer for the write- and seekable `write`
    /// configured with the provided [las::Header]
    /// recommended to use [from_path] for writing to file
    ///
    /// The `bounds` field in the `header` is used as the bounds for the octree
    /// the bounds are checked for being normal
    ///
    /// `max_size` is the maximal number of [las::Point]s an octree node can hold
    /// any max_size < 1 sets the max_size to [crate::MAX_NODE_SIZE_DEFAULT]
    /// this is a soft limit
    ///
    /// `min_size` is the minimal number of [las::Point]s an octree node can hold
    /// any min_size < 1 sets the min_size to [crate::MIN_NODE_SIZE_DEFAULT]
    /// this is a hard limit
    ///
    /// `min_size` greater or equal to `max_size` after checking values < 1
    /// is an InvalidNodeSize error
    ///
    /// [from_path]: Self::from_path
    pub fn new(mut write: W, header: Header, min_size: i32, max_size: i32) -> crate::Result<Self> {
        let start = write.stream_position()?;

        let min_node_size = if min_size < 1 {
            crate::MIN_NODE_SIZE_DEFAULT
        } else {
            min_size
        };

        let max_node_size = if max_size < 1 {
            crate::MAX_NODE_SIZE_DEFAULT
        } else {
            max_size
        };

        if min_node_size >= max_node_size {
            return Err(crate::Error::InvalidNodeSize);
        }

        if header.version() != las::Version::new(1, 4) {
            eprintln!("Old Las version. Upgrading");
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

        // check bounds are normal
        let bounds = header.bounds();
        if !(bounds.max.x - bounds.min.x).is_normal()
            || !(bounds.max.y - bounds.min.y).is_normal()
            || !(bounds.max.z - bounds.min.z).is_normal()
        {
            return Err(crate::Error::InvalidBounds(bounds));
        }

        let mut raw_head = header.into_raw()?;

        // mask off the two leftmost bits corresponding to compression of pdrf
        let pdrf = raw_head.point_data_record_format & 0b00111111;
        let upgrade_pdrf = match pdrf {
            1 => {
                eprintln!("Old point data record format. Upgrading");
                UpgradePdrf::From1to6
            }
            3 => {
                eprintln!("Old point data record format. Upgrading");
                UpgradePdrf::From3to7
            }
            0 | 2 => {
                eprintln!("GPS time is mandatory");
                return Err(las::Error::InvalidPointFormat(las::point::Format::new(
                    raw_head.point_data_record_format,
                )?))?;
            }
            4..=5 | 9.. => {
                eprintln!("Waveform data is not supported");
                return Err(las::Error::InvalidPointFormat(las::point::Format::new(
                    raw_head.point_data_record_format,
                )?))?;
            }
            6..=8 => UpgradePdrf::NoUpgrade,
        };

        // adjust and clear some fields
        raw_head.version = las::Version::new(1, 4);
        raw_head.point_data_record_format += match upgrade_pdrf {
            UpgradePdrf::NoUpgrade => 0,
            UpgradePdrf::From1to6 => 5,
            UpgradePdrf::From3to7 => 4,
        };
        raw_head.point_data_record_format |= 0b10000000; // make sure the compress bits are set
        raw_head.point_data_record_length += match upgrade_pdrf {
            UpgradePdrf::NoUpgrade => 0,
            _ => 2,
        };
        raw_head.number_of_point_records = 0;
        raw_head.number_of_points_by_return = [0; 5];
        raw_head.large_file = None;
        raw_head.evlr = None;
        raw_head.padding = vec![];

        let mut software_buffer = [0_u8; 32];
        for (i, byte) in format!("COPC-rs v{}", crate::VERSION).bytes().enumerate() {
            software_buffer[i] = byte;
        }
        raw_head.generating_software = software_buffer;

        // start building a real header from the raw header
        let mut builder = Builder::new(raw_head)?;
        // add a blank COPC-vlr as the first vlr
        builder.vlrs.push(CopcInfo::default().into_vlr()?);

        // create the laz vlr
        let point_format = builder.point_format;
        let mut laz_items = laz::laszip::LazItemRecordBuilder::new();
        laz_items.add_item(laz::LazItemType::Point14);
        if point_format.has_color {
            if point_format.has_nir {
                laz_items.add_item(laz::LazItemType::RGBNIR14);
            } else {
                laz_items.add_item(laz::LazItemType::RGB14);
            }
        }
        if point_format.extra_bytes > 0 {
            laz_items.add_item(laz::LazItemType::Byte14(point_format.extra_bytes));
        }

        let laz_vlr = laz::LazVlrBuilder::new(laz_items.build())
            .with_variable_chunk_size()
            .build();
        let mut cursor = Cursor::new(Vec::<u8>::new());
        laz_vlr.write_to(&mut cursor)?;
        let laz_vlr = las::Vlr {
            user_id: laz::LazVlr::USER_ID.to_owned(),
            record_id: laz::LazVlr::RECORD_ID,
            description: laz::LazVlr::DESCRIPTION.to_owned(),
            data: cursor.into_inner(),
        };
        builder.vlrs.push(laz_vlr);

        // add the forwarded vlrs
        builder.vlrs.extend(forward_vlrs);
        builder.evlrs.extend(forward_evlrs);
        // the EPT-hierarchy evlr is not yet added

        let header = builder.into_header()?;

        // write the header and vlrs
        // this is just to reserve the space
        header.write_to(&mut write)?;

        let center_point = las::Vector {
            x: (bounds.min.x + bounds.max.x) / 2.,
            y: (bounds.min.y + bounds.max.y) / 2.,
            z: (bounds.min.z + bounds.max.z) / 2.,
        };
        let halfsize = (center_point.x - bounds.min.x)
            .max((center_point.y - bounds.min.y).max(center_point.z - bounds.min.z));

        let mut root_node = OctreeNode::new();

        root_node.bounds = las::Bounds {
            min: las::Vector {
                x: center_point.x - halfsize,
                y: center_point.y - halfsize,
                z: center_point.z - halfsize,
            },
            max: las::Vector {
                x: center_point.x + halfsize,
                y: center_point.y + halfsize,
                z: center_point.z + halfsize,
            },
        };
        root_node.entry.key.level = 0;
        root_node.entry.offset = write.stream_position()?;

        let copc_info = CopcInfo {
            center: center_point,
            halfsize,
            spacing: 0.,
            root_hier_offset: 0,
            root_hier_size: 0,
            gpstime_minimum: f64::MAX,
            gpstime_maximum: f64::MIN,
        };

        Ok(CopcWriter {
            is_closed: false,
            start,
            compressor: CopcCompressor::new(write, header.laz_vlr().unwrap())?,
            header,
            hierarchy: HierarchyPage { entries: vec![] },
            max_node_size,
            copc_info,
            root_node,
            open_chunks: HashMap::default(),
        })
    }

    /// Write anything that implements [IntoIterator]
    /// over [las::Point] to the copc
    ///
    /// returns an `Err`([crate::Error]) if the writer has already been closed
    /// or a point is outside the copc `bounds` or not matching the
    /// [las::point::Format] of the header provided to [new]
    /// [crate::PointAddError::PointAttributesDoNotMatch] take precedence over
    /// [crate::PointAddError::PointNotInBounds]
    ///
    /// All points which both match the point format and are inside the bounds are added
    ///
    /// If all points both match the format and are inside the bounds `Ok(())` is returned
    ///
    /// [new]: Self::new
    pub fn write<D: IntoIterator<Item = las::Point>>(&mut self, data: D) -> crate::Result<()> {
        if self.is_closed {
            return Err(crate::Error::ClosedWriter);
        }

        let mut invalid_points = Ok(());

        for p in data.into_iter() {
            if !p.matches(self.header.point_format()) {
                invalid_points = Err(crate::Error::InvalidPoint(
                    crate::PointAddError::PointAttributesDoNotMatch(*self.header.point_format()),
                ));
                continue;
            }
            if !bounds_contains_point(&self.root_node.bounds, &p) {
                if invalid_points.is_ok() {
                    invalid_points = Err(crate::Error::InvalidPoint(
                        crate::PointAddError::PointNotInBounds,
                    ));
                }
                continue;
            }

            self.add_point(p)?;
        }
        invalid_points
    }

    /// Whether this writer is closed or not
    pub fn is_closed(&self) -> bool {
        self.is_closed
    }

    /// This writer's header, some fields are updated on closing of the writer
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// This writer's EPT Hierarchy, is updated on closing of the writer
    pub fn hierarchy_entries(&self) -> &HierarchyPage {
        &self.hierarchy
    }

    /// This writer's COPC info, is updated on closing of the writer
    pub fn copc_info(&self) -> &CopcInfo {
        &self.copc_info
    }

    /// Close must be called after writing all points
    /// Sometimes it might make sense to call it explictly
    /// but most of the time don't bother calling it
    /// and it will automatically be called on drop
    pub fn close(&mut self) -> crate::Result<()> {
        if self.is_closed {
            return Err(crate::Error::ClosedWriter);
        }
        if self.header.number_of_points() < 1 {
            return Err(crate::Error::EmptyCopcFile);
        }

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

        let start_of_first_evlr = self.compressor.get_mut().stream_position()?;

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

        self.compressor
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
            2. * self.copc_info.halfsize / (self.root_node.entry.point_count as f64);
        self.copc_info.root_hier_offset = start_of_first_evlr + 60; // the header is 60bytes
        self.copc_info.root_hier_size = self.hierarchy.byte_size();

        self.copc_info
            .clone()
            .into_vlr()?
            .into_raw(false)?
            .write_to(self.compressor.get_mut())?;

        self.compressor
            .get_mut()
            .seek(SeekFrom::Start(self.start))?;

        self.is_closed = true;
        Ok(())
    }

    // find the first non-full octree-node that contains the point
    // and add it to the node, if the node now is full
    // add the node to the hierarchy page and write to file
    //
    // this is flawed for non-random ordered iterators
    // as the levels will not contain a representative sample
    // of the entire point cloud
    //
    // should probably add a reshuffle function which is called upon closing
    // which selects a random point and puts it in a other (allowed) node
    // then takes a random point from that node and puts it in a random node
    // and does that for N iterations (N = num_points f.ex)
    // as well as merging tiny leaf nodes with its parent
    //
    // or make the add function probibalistic, but this would probably require
    // to know the number of points to be written upfront to balance the probability
    // and that does not lend itself very well to the possibility of adding many iterators
    fn add_point(&mut self, point: las::Point) -> crate::Result<()> {
        self.header.add_point(&point);

        if point.gps_time.unwrap() < self.copc_info.gpstime_minimum {
            self.copc_info.gpstime_minimum = point.gps_time.unwrap();
        } else if point.gps_time.unwrap() > self.copc_info.gpstime_maximum {
            self.copc_info.gpstime_maximum = point.gps_time.unwrap();
        }

        let mut node_key = None;
        let mut write_chunk = false;

        let root_bounds = self.root_node.bounds;

        // starting from the root walk thorugh the octree
        // and find the correct node to add the point to
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
            } else {
                node_key = Some(node.entry.key.clone());
                node.entry.point_count += 1;

                write_chunk = node.is_full(self.max_node_size);
                break;
            }
        }
        if node_key.is_none() {
            return Err(crate::Error::PointNotAddedToAnyNode);
        }
        let node_key = node_key.unwrap();

        let raw_point = point.into_raw(self.header.transforms())?;

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
            });
        }
        Ok(())
    }
}

impl<W: Write + Seek> Drop for CopcWriter<'_, W> {
    fn drop(&mut self) {
        if !self.is_closed {
            self.close().expect("Error when dropping the writer");
        }
    }
}

#[inline]
fn bounds_contains_point(b: &las::Bounds, p: &las::Point) -> bool {
    !(b.max.x < p.x
        || b.max.y < p.y
        || b.max.z < p.z
        || b.min.x > p.x
        || b.min.y > p.y
        || b.min.z > p.z)
}

enum UpgradePdrf {
    From1to6,
    From3to7,
    NoUpgrade,
}
