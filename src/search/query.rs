use anyhow::Result;

#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub agents: Vec<String>,
}

#[derive(Debug)]
pub struct SearchResult;

pub fn execute(_query: &str, _filters: SearchFilters, _limit: usize) -> Result<Vec<SearchResult>> {
    Ok(Vec::new())
}
