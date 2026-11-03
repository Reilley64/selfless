use crate::ports::EndHlsOutput;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub end_hls_output: EndHlsOutput,
}
