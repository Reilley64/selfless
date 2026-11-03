use crate::browsing::Cursor;
use crate::browsing::Tile;

#[derive(Debug)]
pub struct Output {
    pub tiles: Vec<Tile>,
    pub next: Option<Cursor>,
}
