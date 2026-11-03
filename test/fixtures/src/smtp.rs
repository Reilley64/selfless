use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;

pub struct ScriptedSmtpServer {
    port: u16,
}

impl ScriptedSmtpServer {
    pub const END_OF_DATA: &str = ".";

    pub async fn refusing(command: &'static str, reply: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            writer.write_all(b"220 scripted ESMTP\r\n").await.unwrap();
            let mut in_data = false;
            while let Ok(Some(line)) = lines.next_line().await {
                let upper = line.to_uppercase();
                let answer = if in_data {
                    if line != Self::END_OF_DATA {
                        continue;
                    }
                    in_data = false;
                    if command == Self::END_OF_DATA {
                        format!("{reply}\r\n")
                    } else {
                        "250 queued\r\n".to_owned()
                    }
                } else if upper.starts_with(command) {
                    format!("{reply}\r\n")
                } else if upper.starts_with("EHLO") {
                    "250-scripted\r\n250 AUTH PLAIN LOGIN\r\n".to_owned()
                } else if upper.starts_with("AUTH") {
                    "235 accepted\r\n".to_owned()
                } else if upper.starts_with("DATA") {
                    in_data = true;
                    "354 go on\r\n".to_owned()
                } else if upper.starts_with("QUIT") {
                    "221 bye\r\n".to_owned()
                } else {
                    "250 ok\r\n".to_owned()
                };
                if writer.write_all(answer.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
        Self { port }
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}
