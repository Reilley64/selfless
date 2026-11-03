use mp4_atom::Decode;
use mp4_atom::FourCC;
use mp4_atom::Moov;
use mp4_atom::Trak;
use std::io;
use std::io::SeekFrom;
use std::iter;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncSeekExt;

pub const BOX_TYPES: [&[u8; 4]; 5] = [b"ftyp", b"moov", b"mdat", b"free", b"wide"];

const MAX_MOOV_SIZE: u64 = 64 << 20;

pub async fn keyframes(file: &mut File) -> io::Result<Vec<Duration>> {
    let length = file.metadata().await?.len();
    let mut position = 0;
    while position + 8 <= length {
        file.seek(SeekFrom::Start(position)).await?;
        let mut header = [0; 8];
        file.read_exact(&mut header).await?;
        let size = match u32::from_be_bytes([header[0], header[1], header[2], header[3]]) {
            0 => length - position,
            1 => file.read_u64().await?,
            size => u64::from(size),
        };
        if size < 8 {
            return Err(invalid("bad box size"));
        }
        if &header[4..] == b"moov" {
            if size > MAX_MOOV_SIZE {
                return Err(invalid("moov box too large to read"));
            }
            file.seek(SeekFrom::Start(position)).await?;
            let mut moov = vec![0; size as usize];
            file.read_exact(&mut moov).await?;
            let moov = Moov::decode(&mut moov.as_slice())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            return Ok(moov
                .trak
                .iter()
                .find(|x| x.mdia.hdlr.handler == FourCC::new(b"vide"))
                .map(|x| sync_sample_times(x, moov.mvhd.timescale))
                .unwrap_or_default());
        }
        position += size;
    }
    Ok(Vec::new())
}

fn sync_sample_times(trak: &Trak, movie_timescale: u32) -> Vec<Duration> {
    let timescale = u128::from(trak.mdia.mdhd.timescale);
    let stbl = &trak.mdia.minf.stbl;
    let decode_times = stbl
        .stts
        .entries
        .iter()
        .flat_map(|x| iter::repeat_n(i128::from(x.sample_delta), x.sample_count as usize))
        .scan(0, |time, delta| {
            let start = *time;
            *time += delta;
            Some(start)
        });
    let offsets = stbl
        .ctts
        .iter()
        .flat_map(|x| &x.entries)
        .flat_map(|x| iter::repeat_n(i128::from(x.sample_offset), x.sample_count as usize))
        .chain(iter::repeat(0));
    let shift = edit_shift(trak, movie_timescale);
    let times = decode_times
        .zip(offsets)
        .map(|(decode_time, offset)| decode_time + offset + shift)
        .collect::<Vec<_>>();
    let mut keyframes = match &stbl.stss {
        Some(stss) => stss
            .entries
            .iter()
            .filter_map(|x| times.get((*x as usize).checked_sub(1)?).copied())
            .collect(),
        None => times,
    }
    .into_iter()
    .filter_map(|x| u128::try_from(x).ok())
    .map(|x| Duration::from_nanos((x * 1_000_000_000 / timescale) as u64))
    .collect::<Vec<_>>();
    keyframes.sort();
    keyframes
}

fn edit_shift(trak: &Trak, movie_timescale: u32) -> i128 {
    let Some(elst) = trak.edts.as_ref().and_then(|x| x.elst.as_ref()) else {
        return 0;
    };
    let empty = elst
        .entries
        .iter()
        .take_while(|x| x.media_time.is_none())
        .map(|x| i128::from(x.segment_duration))
        .sum::<i128>()
        * i128::from(trak.mdia.mdhd.timescale)
        / i128::from(movie_timescale.max(1));
    let media_time = elst
        .entries
        .iter()
        .find_map(|x| x.media_time)
        .map_or(0, i128::from);
    empty - media_time
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
