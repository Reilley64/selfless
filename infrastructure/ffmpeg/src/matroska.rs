use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::io::SeekFrom;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncSeekExt;

const SEGMENT: u32 = 0x1853_8067;
const SEEK_HEAD: u32 = 0x114D_9B74;
const SEEK: u32 = 0x4DBB;
const SEEK_ID: u32 = 0x53AB;
const SEEK_POSITION: u32 = 0x53AC;
const INFO: u32 = 0x1549_A966;
const TIMESTAMP_SCALE: u32 = 0x2A_D7B1;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_NUMBER: u32 = 0xD7;
const TRACK_TYPE: u32 = 0x83;
const CUES: u32 = 0x1C53_BB6B;
const CUE_POINT: u32 = 0xBB;
const CUE_TIME: u32 = 0xB3;
const CUE_TRACK_POSITIONS: u32 = 0xB7;
const CUE_TRACK: u32 = 0xF7;
const CLUSTER: u32 = 0x1F43_B675;

const VIDEO_TRACK_TYPE: u64 = 1;
const DEFAULT_TIMESTAMP_SCALE: u64 = 1_000_000;
const MAX_ELEMENT_SIZE: u64 = 64 << 20;

struct Header {
    id: u32,
    data: u64,
    size: Option<u64>,
}

pub async fn keyframes(file: &mut File) -> io::Result<Vec<Duration>> {
    let ebml = header_at(file, 0)
        .await?
        .ok_or_else(|| invalid("empty file"))?;
    let segment = header_at(file, end(&ebml)?)
        .await?
        .filter(|x| x.id == SEGMENT)
        .ok_or_else(|| invalid("no Matroska segment"))?;

    let mut elements = HashMap::new();
    let mut seek_heads = Vec::new();
    let mut position = segment.data;
    while let Some(header) = header_at(file, position).await? {
        if header.id == CLUSTER {
            break;
        }
        match header.id {
            SEEK_HEAD => seek_heads.push(body(file, &header).await?),
            INFO | TRACKS | CUES => {
                elements.insert(header.id, body(file, &header).await?);
            }
            _ => {}
        }
        position = end(&header)?;
    }

    let mut visited = HashSet::new();
    while let Some(seek_head) = seek_heads.pop() {
        for (id, offset) in seek_entries(&seek_head)? {
            let wanted = id == SEEK_HEAD
                || (matches!(id, INFO | TRACKS | CUES) && !elements.contains_key(&id));
            if !wanted || !visited.insert(offset) {
                continue;
            }
            let Some(header) = header_at(file, segment.data + offset)
                .await?
                .filter(|x| x.id == id)
            else {
                continue;
            };
            let data = body(file, &header).await?;
            if id == SEEK_HEAD {
                seek_heads.push(data);
            } else {
                elements.insert(id, data);
            }
        }
    }

    let scale = match elements.get(&INFO) {
        Some(info) => children(info)?
            .into_iter()
            .find(|(id, _)| *id == TIMESTAMP_SCALE)
            .map(|(_, data)| uint(data))
            .unwrap_or(DEFAULT_TIMESTAMP_SCALE),
        None => DEFAULT_TIMESTAMP_SCALE,
    };
    let (Some(tracks), Some(cues)) = (elements.get(&TRACKS), elements.get(&CUES)) else {
        return Ok(Vec::new());
    };
    let Some(video_track) = video_track(tracks)? else {
        return Ok(Vec::new());
    };
    cue_times(cues, video_track, scale)
}

fn video_track(tracks: &[u8]) -> io::Result<Option<u64>> {
    for (id, entry) in children(tracks)? {
        if id != TRACK_ENTRY {
            continue;
        }
        let fields = children(entry)?;
        let field = |wanted| {
            fields
                .iter()
                .find(|(id, _)| *id == wanted)
                .map(|(_, data)| uint(data))
        };
        if field(TRACK_TYPE) == Some(VIDEO_TRACK_TYPE) {
            return Ok(field(TRACK_NUMBER));
        }
    }
    Ok(None)
}

fn cue_times(cues: &[u8], track: u64, scale: u64) -> io::Result<Vec<Duration>> {
    let mut times = Vec::new();
    for (id, point) in children(cues)? {
        if id != CUE_POINT {
            continue;
        }
        let mut time = None;
        let mut on_track = false;
        for (id, data) in children(point)? {
            match id {
                CUE_TIME => time = Some(uint(data)),
                CUE_TRACK_POSITIONS => {
                    on_track |= children(data)?
                        .iter()
                        .any(|(id, data)| *id == CUE_TRACK && uint(data) == track);
                }
                _ => {}
            }
        }
        if let (Some(time), true) = (time, on_track) {
            times.push(Duration::from_nanos(
                (u128::from(time) * u128::from(scale)) as u64,
            ));
        }
    }
    times.sort();
    times.dedup();
    Ok(times)
}

fn seek_entries(seek_head: &[u8]) -> io::Result<Vec<(u32, u64)>> {
    let mut entries = Vec::new();
    for (id, seek) in children(seek_head)? {
        if id != SEEK {
            continue;
        }
        let fields = children(seek)?;
        let id = fields.iter().find(|(id, _)| *id == SEEK_ID);
        let position = fields.iter().find(|(id, _)| *id == SEEK_POSITION);
        if let (Some((_, id)), Some((_, position))) = (id, position) {
            entries.push((uint(id) as u32, uint(position)));
        }
    }
    Ok(entries)
}

fn children(data: &[u8]) -> io::Result<Vec<(u32, &[u8])>> {
    let mut children = Vec::new();
    let mut rest = data;
    while !rest.is_empty() {
        let (id, id_length) = vint(rest, true).ok_or_else(|| invalid("bad element ID"))?;
        let (size, size_length) =
            vint(&rest[id_length..], false).ok_or_else(|| invalid("bad element size"))?;
        let start = id_length + size_length;
        let data = usize::try_from(size)
            .ok()
            .and_then(|size| rest.get(start..start.checked_add(size)?))
            .ok_or_else(|| invalid("element past its parent"))?;
        children.push((id as u32, data));
        rest = &rest[start + data.len()..];
    }
    Ok(children)
}

async fn header_at(file: &mut File, position: u64) -> io::Result<Option<Header>> {
    file.seek(SeekFrom::Start(position)).await?;
    let mut buffer = [0; 12];
    let mut length = 0;
    while length < buffer.len() {
        match file.read(&mut buffer[length..]).await? {
            0 => break,
            read => length += read,
        }
    }
    if length == 0 {
        return Ok(None);
    }
    let buffer = &buffer[..length];
    let (id, id_length) = vint(buffer, true)
        .filter(|(_, length)| *length <= 4)
        .ok_or_else(|| invalid("bad element ID"))?;
    let (size, size_length) =
        vint(&buffer[id_length..], false).ok_or_else(|| invalid("bad element size"))?;
    let unknown = size == (1 << (7 * size_length)) - 1;
    Ok(Some(Header {
        id: id as u32,
        data: position + (id_length + size_length) as u64,
        size: (!unknown).then_some(size),
    }))
}

async fn body(file: &mut File, header: &Header) -> io::Result<Vec<u8>> {
    let size = header
        .size
        .filter(|x| *x <= MAX_ELEMENT_SIZE)
        .ok_or_else(|| invalid("element too large to read"))?;
    file.seek(SeekFrom::Start(header.data)).await?;
    let mut data = vec![0; size as usize];
    file.read_exact(&mut data).await?;
    Ok(data)
}

fn end(header: &Header) -> io::Result<u64> {
    header
        .size
        .map(|x| header.data + x)
        .ok_or_else(|| invalid("element of unknown size"))
}

fn vint(data: &[u8], keep_marker: bool) -> Option<(u64, usize)> {
    let first = *data.first()?;
    let length = first.leading_zeros() as usize + 1;
    let bytes = data.get(..length.min(8))?;
    let first = if keep_marker {
        u64::from(first)
    } else {
        u64::from(first) & (0xFF >> length)
    };
    let value = bytes[1..]
        .iter()
        .fold(first, |value, x| value << 8 | u64::from(*x));
    (length <= 8).then_some((value, length))
}

fn uint(data: &[u8]) -> u64 {
    data.iter().fold(0, |value, x| value << 8 | u64::from(*x))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
