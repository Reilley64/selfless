#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum PlayMethod {
    DirectPlay,
    Remux,
    Transcode,
}

impl PlayMethod {
    pub fn uses_hls(self) -> bool {
        self != Self::DirectPlay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remux_and_transcode_use_hls_and_direct_play_does_not() {
        assert_eq!(
            [
                PlayMethod::DirectPlay,
                PlayMethod::Remux,
                PlayMethod::Transcode
            ]
            .map(PlayMethod::uses_hls),
            [false, true, true]
        );
    }
}
