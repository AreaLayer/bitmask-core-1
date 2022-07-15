#![allow(clippy::unused_unit)]
use std::str::FromStr;

use anyhow::{format_err, Result};
use bdk::{wallet::AddressIndex::LastUnused, BlockTime};
use bitcoin::{util::address::Address, OutPoint};
use bitcoin::{Transaction, Txid};
use serde::{Deserialize, Serialize};
use serde_encrypt::{
    serialize::impls::BincodeSerializer, shared_key::SharedKey, traits::SerdeEncryptSharedKey,
    AsSharedKey, EncryptedMessage,
};
use sha2::{Digest, Sha256};

mod data;
mod operations;
mod util;
#[cfg(target_arch = "wasm32")]
pub mod web;

use data::{
    constants,
    structs::{Asset, SatsInvoice, ThinAsset, TransferResponse},
};

use operations::{
    bitcoin::{create_transaction, get_mnemonic, get_wallet, save_mnemonic},
    rgb::{
        accept_transfer, blind_utxo, get_asset_by_contract_id, get_asset_by_genesis, get_assets,
        issue_asset, transfer_asset, validate_transfer, Genesis, OwnedValue,
    },
};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VaultData {
    pub btc_descriptor: String,
    pub btc_change_descriptor: String,
    pub rgb_tokens_descriptor: String,
    pub rgb_nfts_descriptor: String,
    pub pubkey_hash: String,
}

impl SerdeEncryptSharedKey for VaultData {
    type S = BincodeSerializer<Self>; // you can specify serializer implementation (or implement it by yourself).
}

pub fn get_vault(password: &str, encrypted_descriptors: &str) -> Result<VaultData> {
    let mut hasher = Sha256::new();

    // write input message
    hasher.update(password.as_bytes());

    // read hash digest and consume hasher
    let result = hasher.finalize();
    let shared_key: [u8; 32] = result
        .as_slice()
        .try_into()
        .expect("slice with incorrect length");
    let encrypted_descriptors: Vec<u8> = serde_json::from_str(encrypted_descriptors).unwrap();
    // STORAGE_KEY_DESCRIPTOR_ENCRYPTED
    let encrypted_message = EncryptedMessage::deserialize(encrypted_descriptors);
    match encrypted_message {
        Ok(encrypted_message) => {
            let vault_data =
                VaultData::decrypt_owned(&encrypted_message, &SharedKey::from_array(shared_key));
            match vault_data {
                Ok(vault_data) => Ok(vault_data),
                Err(e) => Err(format_err!("Error: {e}")),
            }
        }
        Err(e) => Err(format_err!("Error: {e}")),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MnemonicSeedData {
    pub mnemonic: String,
    pub serialized_encrypted_message: Vec<u8>,
}

pub fn get_mnemonic_seed(
    encryption_password: &str,
    seed_password: &str,
) -> Result<MnemonicSeedData> {
    let mut hasher = Sha256::new();

    // write input message
    hasher.update(encryption_password.as_bytes());

    // read hash digest and consume hasher
    let hash = hasher.finalize();
    let shared_key: [u8; 32] = hash
        .as_slice()
        .try_into()
        .expect("slice with incorrect length");

    let (
        mnemonic,
        btc_descriptor,
        btc_change_descriptor,
        rgb_tokens_descriptor,
        rgb_nfts_descriptor,
        pubkey_hash,
    ) = get_mnemonic(seed_password);
    let vault_data = VaultData {
        btc_descriptor,
        btc_change_descriptor,
        rgb_tokens_descriptor,
        rgb_nfts_descriptor,
        pubkey_hash,
    };
    let encrypted_message = vault_data
        .encrypt(&SharedKey::from_array(shared_key))
        .unwrap();
    let serialized_encrypted_message: Vec<u8> = encrypted_message.serialize();
    let mnemonic_seed_data = MnemonicSeedData {
        mnemonic,
        serialized_encrypted_message,
    };

    Ok(mnemonic_seed_data)
}

pub fn save_mnemonic_seed(
    mnemonic: &str,
    encryption_password: &str,
    seed_password: &str,
) -> Result<MnemonicSeedData> {
    let mut hasher = Sha256::new();

    // write input message
    hasher.update(encryption_password.as_bytes());

    // read hash digest and consume hasher
    let hash = hasher.finalize();
    let shared_key: [u8; 32] = hash
        .as_slice()
        .try_into()
        .expect("slice with incorrect length");

    let (
        btc_descriptor,
        btc_change_descriptor,
        rgb_tokens_descriptor,
        rgb_nfts_descriptor,
        pubkey_hash,
    ) = save_mnemonic(seed_password, mnemonic);
    let vault_data = VaultData {
        btc_descriptor,
        btc_change_descriptor,
        rgb_tokens_descriptor,
        rgb_nfts_descriptor,
        pubkey_hash,
    };
    let encrypted_message = vault_data
        .encrypt(&SharedKey::from_array(shared_key))
        .unwrap();
    let serialized_encrypted_message: Vec<u8> = encrypted_message.serialize();
    let mnemonic_seed_data = MnemonicSeedData {
        mnemonic: mnemonic.to_owned(),
        serialized_encrypted_message,
    };

    Ok(mnemonic_seed_data)
}

#[derive(Serialize, Deserialize)]
pub struct WalletData {
    pub address: String,
    pub balance: String,
    pub transactions: Vec<WalletTransaction>,
    pub unspent: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct WalletTransaction {
    pub txid: Txid,
    pub received: u64,
    pub sent: u64,
    pub fee: Option<u64>,
    pub confirmed: bool,
    pub confirmation_time: Option<BlockTime>,
}

pub async fn get_wallet_data(
    descriptor: &str,
    change_descriptor: Option<&str>,
) -> Result<WalletData> {
    log!("get_wallet_data");
    log!(&descriptor, format!("{:?}", &change_descriptor));

    let wallet = get_wallet(descriptor, change_descriptor).await;
    let address = wallet
        .as_ref()
        .unwrap()
        .get_address(LastUnused)
        .unwrap()
        .to_string();
    log!(&address);
    let balance = wallet.as_ref().unwrap().get_balance().unwrap().to_string();
    log!(&balance);
    let unspent = wallet.as_ref().unwrap().list_unspent().unwrap_or_default();
    let unspent: Vec<String> = unspent
        .into_iter()
        .map(|x| x.outpoint.to_string())
        .collect();
    log!(format!("unspent: {unspent:#?}"));

    let transactions = wallet
        .as_ref()
        .unwrap()
        .list_transactions(false)
        .unwrap_or_default();
    log!(format!("transactions: {transactions:#?}"));

    let transactions: Vec<WalletTransaction> = transactions
        .into_iter()
        .map(|tx| WalletTransaction {
            txid: tx.txid,
            received: tx.received,
            sent: tx.sent,
            fee: tx.fee,
            confirmed: tx.confirmation_time.is_some(),
            confirmation_time: tx.confirmation_time,
        })
        .collect();

    Ok(WalletData {
        address,
        balance,
        transactions,
        unspent,
    })
}

pub async fn import_list_assets(node_url: Option<String>) -> Result<Vec<Asset>> {
    log!("import_list_assets");
    let assets = get_assets(node_url).await?;
    log!(format!("get assets: {assets:#?}"));
    Ok(assets)
}

pub fn create_asset(
    ticker: &str,
    name: &str,
    precision: u8,
    supply: u64,
    utxo: &str,
) -> Result<(Genesis, Vec<OwnedValue>)> {
    let utxo = OutPoint::from_str(utxo)?;
    issue_asset(ticker, name, precision, supply, utxo)
}

pub async fn import_asset(
    rgb_tokens_descriptor: &str,
    contract_id: Option<&str>,
    genesis: Option<&str>,
    node_url: Option<String>,
) -> Result<ThinAsset> {
    match genesis {
        Some(genesis) => get_asset_by_genesis(genesis).await,
        None => match contract_id {
            Some(contract_id) => {
                let wallet = get_wallet(rgb_tokens_descriptor, None).await;
                let unspent = wallet.as_ref().unwrap().list_unspent().unwrap_or_default();

                log!(format!("getting asset by contract id, {contract_id}"));
                let asset = get_asset_by_contract_id(contract_id, unspent, node_url).await;
                log!(format!("get asset {asset:?}"));
                match asset {
                    Ok(asset) => Ok(asset),
                    Err(e) => Err(format_err!("Server error: {e}")),
                }
            }
            None => {
                log!("genesis....");
                Err(format_err!("Error: Unknown error in import_asset"))
            }
        },
    }
}

#[derive(Serialize, Deserialize)]
struct TransactionData {
    blinding: String,
    utxo: OutPoint,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlindingUtxo {
    conceal: String,
    blinding: String,
    utxo: OutPoint,
}

pub fn set_blinded_utxo(utxo_string: &str) -> Result<BlindingUtxo> {
    let mut split = utxo_string.split(':');
    let utxo = OutPoint {
        txid: Txid::from_str(split.next().unwrap())?,
        vout: split.next().unwrap().to_string().parse::<u32>()?,
    };
    let (blind, utxo) = blind_utxo(utxo)?;

    let blinding_utxo = BlindingUtxo {
        conceal: blind.conceal,
        blinding: blind.blinding,
        utxo,
    };

    Ok(blinding_utxo)
}

pub async fn send_sats(
    descriptor: &str,
    change_descriptor: &str,
    address: String,
    amount: u64,
) -> Result<Transaction> {
    let address = Address::from_str(&(address));

    let wallet = get_wallet(descriptor, Some(change_descriptor))
        .await
        .unwrap();

    let transaction = create_transaction(
        vec![SatsInvoice {
            address: address.unwrap(),
            amount,
        }],
        &wallet,
    )
    .await?;

    Ok(transaction)
}

#[derive(Serialize, Deserialize)]
pub struct FundVaultDetails {
    pub txid: String,
    pub send_assets: String,
    pub recv_assets: String,
    pub send_udas: String,
    pub recv_udas: String,
}

pub async fn fund_wallet(
    descriptor: &str,
    change_descriptor: &str,
    address: &str,
    uda_address: &str,
) -> Result<FundVaultDetails> {
    let address = Address::from_str(address);
    let uda_address = Address::from_str(uda_address);

    let wallet = get_wallet(descriptor, Some(change_descriptor))
        .await
        .unwrap();
    let invoice = SatsInvoice {
        address: address.unwrap(),
        amount: 613,
    };
    let uda_invoice = SatsInvoice {
        address: uda_address.unwrap(),
        amount: 613,
    };
    let details = create_transaction(
        vec![invoice.clone(), invoice, uda_invoice.clone(), uda_invoice],
        &wallet,
    )
    .await?;

    let txid = details.txid();
    let outputs: Vec<String> = details
        .output
        .iter()
        .enumerate()
        .map(|(i, _)| format!("{txid}:{i}"))
        .collect();

    Ok(FundVaultDetails {
        txid: txid.to_string(),
        send_assets: outputs[0].clone(),
        recv_assets: outputs[1].clone(),
        send_udas: outputs[2].clone(),
        recv_udas: outputs[3].clone(),
    })
}

pub async fn send_tokens(
    btc_descriptor: &str,
    btc_change_descriptor: &str,
    rgb_tokens_descriptor: &str,
    blinded_utxo: String,
    amount: u64,
    asset: ThinAsset,
    node_url: Option<String>,
) -> Result<TransferResponse> {
    let assets_wallet = get_wallet(rgb_tokens_descriptor, None).await.unwrap();
    let full_wallet = get_wallet(rgb_tokens_descriptor, Some(btc_descriptor))
        .await
        .unwrap();
    let full_change_wallet = get_wallet(rgb_tokens_descriptor, Some(btc_change_descriptor))
        .await
        .unwrap();
    let consignment = transfer_asset(
        blinded_utxo,
        amount,
        asset,
        &full_wallet,
        &full_change_wallet,
        &assets_wallet,
        node_url,
    )
    .await?;

    Ok(consignment)
}

pub async fn validate_transaction(consignment: &str, node_url: Option<String>) -> Result<()> {
    validate_transfer(consignment.to_owned(), node_url).await
}

pub async fn accept_transaction(
    consignment: &str,
    txid: &str,
    vout: u32,
    blinding: &str,
    node_url: Option<String>,
) -> Result<String> {
    let txid = Txid::from_str(txid)?;

    let transaction_data = TransactionData {
        blinding: blinding.to_owned(),
        utxo: OutPoint { txid, vout },
    };
    let accept = accept_transfer(
        consignment.to_owned(),
        transaction_data.utxo,
        transaction_data.blinding,
        node_url,
    )
    .await?;
    log!("hola denueveo 3");
    Ok(accept)
}

pub async fn import_accept(
    rgb_tokens_descriptor: &str,
    asset: &str,
    consignment: &str,
    txid: &str,
    vout: u32,
    blinding: String,
    node_url: Option<String>,
) -> Result<ThinAsset> {
    let txid = Txid::from_str(txid)?;

    let transaction_data = TransactionData {
        blinding,
        utxo: OutPoint { txid, vout },
    };

    let accept = accept_transfer(
        consignment.to_owned(),
        transaction_data.utxo,
        transaction_data.blinding,
        node_url.clone(),
    )
    .await;
    match accept {
        Ok(_accept) => {
            let wallet = get_wallet(rgb_tokens_descriptor, None).await;
            let unspent = wallet.as_ref().unwrap().list_unspent().unwrap_or_default();
            let asset = get_asset_by_contract_id(&asset, unspent, node_url).await;
            log!(format!("get asset {asset:#?}"));
            asset
        }
        Err(e) => Err(e),
    }
}

pub fn switch_network(network_str: &str) {
    constants::switch_network(network_str);
}

pub fn get_network() -> Result<String> {
    match constants::NETWORK.read() {
        Ok(network) => Ok(network.to_string()),
        Err(err) => Ok(err.to_string()),
    }
}
