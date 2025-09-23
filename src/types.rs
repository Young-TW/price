use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Portfolio(pub HashMap<String, HashMap<String, f64>>);

#[derive(Debug, Deserialize)]
pub struct ApiKeys(pub HashMap<String, String>);
