use anyhow::{Error, Result};
use rgb_core::Genesis;

use crate::{
    data::{
        constants::url,
        structs::{Allocation, Asset, ExportRequestMini, ThinAsset},
    },
    log,
    util::{get, post_json},
};

pub async fn get_asset_by_genesis(genesis: &str) -> Result<ThinAsset> {
    let genesis: Genesis = serde_json::from_str(genesis)?;

    todo!();
}

pub async fn get_asset_by_contract_id(
    asset: &str,
    unspent: Vec<bdk::LocalUtxo>,
    node_url: Option<String>,
) -> Result<ThinAsset> {
    let asset_data = ExportRequestMini {
        asset: asset.to_owned(),
    };
    let (response, _) = match post_json(url("getasset", &node_url), &asset_data).await {
        Ok(response) => response,
        Err(e) => return Err(Error::msg(e)),
    };
    log!(format!("response: {response:#?}"));
    let assets: Vec<Asset> = serde_json::from_str(&response)?;
    if assets.is_empty() {
        return Err(Error::msg("Incorrect rgb id".to_string()));
    }
    let allocations: Vec<Allocation> = assets[0]
        .known_allocations
        .clone()
        .into_iter()
        .filter(|a| {
            unspent
                .clone()
                .into_iter()
                .any(|y| y.outpoint.to_string().eq(&a.outpoint))
        })
        .collect();
    log!(format!("allocations: {allocations:#?}"));
    let amount = allocations
        .clone()
        .into_iter()
        .map(|a| a.revealed_amount.value)
        .reduce(|a, b| a + b);
    log!(format!("amount: {amount:#?}"));
    let thin_assets = ThinAsset {
        id: asset.to_owned(),
        ticker: assets[0].ticker.clone(),
        name: assets[0].name.clone(),
        description: assets[0].description.clone().unwrap(),
        allocations,
        balance: Some(amount.unwrap_or_default()),
        dolar_balance: None,
    };

    log!(format!("thin_assets: {thin_assets:?}"));
    Ok(thin_assets)
}

pub async fn get_assets(node_url: Option<String>) -> Result<Vec<Asset>> {
    let (response, _) = get(url("list", &node_url)).await?;
    log!(format!("listassets: {response:#?}"));
    let assets: Vec<Asset> = serde_json::from_str(&response)?;
    Ok(assets)
}
