use anyhow::{bail, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::ZlibDecoder;
use std::io::{Cursor, Read};

pub const PACKAGE_FILE_TAG: u32 = 0x9E2A83C1;

/// Result of decompressing a Satisfactory save file.
pub struct DecompressedSave {
    pub uncompressed_data: Vec<u8>,
}

/// Decompresses chunks starting at `header_end_offset` in raw save bytes.
pub fn decompress_save_chunks(data: &[u8], header_end_offset: usize) -> Result<DecompressedSave> {
    let mut offset = header_end_offset;
    let data_len = data.len();

    // Estimate initial capacity
    let estimated_capacity = (data_len - offset).saturating_mul(4);
    let mut decompressed = Vec::with_capacity(estimated_capacity);

    while offset < data_len {
        if offset + 49 > data_len {
            bail!(
                "Unexpected end of file while reading chunk header at offset {}",
                offset
            );
        }

        let mut rdr = Cursor::new(&data[offset..offset + 49]);
        let signature = rdr.read_u32::<LittleEndian>()?;
        if signature != PACKAGE_FILE_TAG {
            bail!(
                "Invalid chunk package signature 0x{:08X} at offset {}, expected 0x{:08X}",
                signature,
                offset,
                PACKAGE_FILE_TAG
            );
        }

        let _tag2 = rdr.read_u32::<LittleEndian>()?;
        let _tag3 = rdr.read_u8()?;
        let _max_chunk_size = rdr.read_u32::<LittleEndian>()?;
        let _tag4 = rdr.read_u32::<LittleEndian>()?; // expected 0x03000000

        let comp_len1 = rdr.read_u64::<LittleEndian>()? as usize;
        let uncomp_len1 = rdr.read_u64::<LittleEndian>()? as usize;
        let comp_len2 = rdr.read_u64::<LittleEndian>()? as usize;
        let uncomp_len2 = rdr.read_u64::<LittleEndian>()? as usize;

        if comp_len1 != comp_len2 {
            bail!(
                "Chunk compressed length mismatch: {} != {}",
                comp_len1,
                comp_len2
            );
        }
        if uncomp_len1 != uncomp_len2 {
            bail!(
                "Chunk uncompressed length mismatch: {} != {}",
                uncomp_len1,
                uncomp_len2
            );
        }

        offset += 49;

        if offset + comp_len1 > data_len {
            bail!(
                "Compressed chunk of {} bytes exceeds data size {} at offset {}",
                comp_len1,
                data_len,
                offset
            );
        }

        // Decompress chunk using ZlibDecoder
        let chunk_slice = &data[offset..offset + comp_len1];
        let prev_len = decompressed.len();
        decompressed.resize(prev_len + uncomp_len1, 0);

        let mut decoder = ZlibDecoder::new(chunk_slice);
        decoder
            .read_exact(&mut decompressed[prev_len..])
            .with_context(|| {
                format!(
                    "Failed to decompress zlib chunk of size {} at offset {}",
                    comp_len1, offset
                )
            })?;

        offset += comp_len1;
    }

    if decompressed.len() < 8 {
        bail!("Decompressed data too short to contain size header");
    }

    // First 8 bytes of uncompressed data are total uncompressed payload size
    let reported_uncompressed_size =
        (&decompressed[0..8]).read_u64::<LittleEndian>()? as usize + 8;
    if reported_uncompressed_size <= decompressed.len() {
        decompressed.truncate(reported_uncompressed_size);
    }

    Ok(DecompressedSave {
        uncompressed_data: decompressed,
    })
}
