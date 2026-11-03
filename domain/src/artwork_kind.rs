#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum ArtworkKind {
    Poster,
    Fanart,
    Clearlogo,
    Thumb,
}

impl ArtworkKind {
    pub fn widths(self) -> &'static [i64] {
        match self {
            Self::Poster => &[],
            Self::Fanart => &[480, 960, 1280, 1920],
            Self::Clearlogo => &[300, 600],
            Self::Thumb => &[480, 960],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fanart_comes_at_tile_and_backdrop_widths() {
        assert_eq!(ArtworkKind::Fanart.widths(), [480, 960, 1280, 1920]);
    }

    #[test]
    fn thumbs_come_at_tile_widths_and_clearlogos_at_logo_widths() {
        assert_eq!(ArtworkKind::Thumb.widths(), [480, 960]);
        assert_eq!(ArtworkKind::Clearlogo.widths(), [300, 600]);
    }

    #[test]
    fn posters_are_not_served() {
        assert_eq!(ArtworkKind::Poster.widths(), [] as [i64; 0]);
    }
}
