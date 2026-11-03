use crate::matroska;
use crate::mp4;
use std::io;
use std::path::Path;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

const EBML_MAGIC: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3];

pub async fn read_keyframes(path: &Path) -> io::Result<Vec<Duration>> {
    let mut file = File::open(path).await?;
    let mut magic = [0; 4];
    file.read_exact(&mut magic).await?;
    match magic {
        EBML_MAGIC => matroska::keyframes(&mut file).await,
        _ => {
            let mut box_type = [0; 4];
            file.read_exact(&mut box_type).await?;
            if mp4::BOX_TYPES.contains(&&box_type) {
                mp4::keyframes(&mut file).await
            } else {
                Ok(Vec::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read_keyframes;
    use serde::Deserialize;
    use std::path::Path;
    use std::time::Duration;
    use tokio::process::Command;

    #[derive(Deserialize)]
    struct Output {
        streams: [Stream; 1],
        packets: Vec<Packet>,
    }

    #[derive(Deserialize)]
    struct Stream {
        time_base: String,
    }

    #[derive(Deserialize)]
    struct Packet {
        pts: u64,
        flags: String,
    }

    async fn ffprobe_keyframes(path: &Path) -> Vec<Duration> {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=time_base:packet=pts,flags",
                "-of",
                "json",
            ])
            .arg(path)
            .output()
            .await
            .expect("ffprobe runs");
        let output = serde_json::from_slice::<Output>(&output.stdout).expect("ffprobe prints JSON");
        let (numerator, denominator) = output.streams[0]
            .time_base
            .split_once('/')
            .expect("the time base is a fraction");
        let numerator = numerator.parse::<u128>().unwrap();
        let denominator = denominator.parse::<u128>().unwrap();
        let mut keyframes = output
            .packets
            .iter()
            .filter(|x| x.flags.starts_with('K'))
            .map(|x| {
                Duration::from_nanos(
                    (u128::from(x.pts) * numerator * 1_000_000_000 / denominator) as u64,
                )
            })
            .collect::<Vec<_>>();
        keyframes.sort();
        keyframes
    }

    fn fixture(name: &str) -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/fixtures/media")
            .join(name)
            .leak()
    }

    #[tokio::test]
    async fn the_keyframes_of_a_matroska_file_come_from_its_cues_and_match_ffprobe() {
        let path = fixture("keyframes.mkv");

        let keyframes = read_keyframes(path).await.unwrap();

        assert_eq!(keyframes.len(), 10);
        assert_eq!(keyframes, ffprobe_keyframes(path).await);
    }

    #[tokio::test]
    async fn the_keyframes_of_an_mp4_file_come_from_its_stss_box_and_match_ffprobe() {
        let path = fixture("keyframes.mp4");

        let keyframes = read_keyframes(path).await.unwrap();

        assert_eq!(keyframes.len(), 10);
        assert_eq!(keyframes, ffprobe_keyframes(path).await);
    }
}
