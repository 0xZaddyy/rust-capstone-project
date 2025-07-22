#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoin::sighash::Prevouts;
use bitcoincore_rpc::bitcoin::{Address, Amount, Network};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use jsonrpc::Error;
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(RPC_URL, Auth::UserPass(RPC_USER.into(), RPC_PASS.into()))?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {blockchain_info:?}");

    // Create/Load the wallets, named 'Miner' and 'Trader'. Have logic to optionally create/load them if they do not exist or not loaded already.
    let _ = rpc.create_wallet("Miner", None, None, None, None);
    let _ = rpc.create_wallet("Trader", None, None, None, None);

    let miner_rpc = Client::new(
        "http://127.0.0.1:18443/wallet/Miner",
        Auth::UserPass(RPC_USER.into(), RPC_PASS.into()),
    )?;
    let trader_rpc = Client::new(
        "http://127.0.0.1:18443/wallet/Trader",
        Auth::UserPass(RPC_USER.into(), RPC_PASS.into()),
    )?;

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    let miner_mine_addr = miner_rpc
        .get_new_address("Mining Reward".into(), None)?
        .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
        .unwrap();
    // Mine 101 blocks to mature coinbase transaction
    let blocks = rpc.generate_to_address(101, &miner_mine_addr)?;

    // Coinbase transactions require 100 confirmations before it can be spent
    // 101 blocks was mined to ensure the reward can be available

    // print Miner wallet balance
    let miner_balance = miner_rpc.get_balance(None, None)?;
    println!("Miner Balance: {:.8} BTC", miner_balance.to_btc());

    // Load Trader wallet and generate a new address
    let trader_address = trader_rpc
        .get_new_address("Received".into(), None)?
        .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
        .unwrap();

    // Send 20 BTC from Miner to Trader
    let amount_to_send = Amount::from_btc(20.0)?;
    let txid = miner_rpc.send_to_address(
        &trader_address,
        amount_to_send,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    // Check transaction in mempool
    let mempool_entry = rpc.get_mempool_entry(&txid)?;
    println!("Unconfirmed transaction: {mempool_entry:#?}");

    // Mine 1 block to confirm the transaction
    let blockhash = rpc.generate_to_address(1, &miner_mine_addr)?[0];
    let block_info = rpc.get_block_header_info(&blockhash)?;
    let block_height = block_info.height;

    // Extract all required transaction details
    let tx_info = miner_rpc.get_transaction(&txid, Some(true))?;
    let decoded = miner_rpc.get_raw_transaction(&txid, Some(&blockhash))?;

    let vin = &decoded.input[0];
    let prev_txid = vin.previous_output.txid;
    let prev_vout_index = vin.previous_output.vout;
    let prev_tx = miner_rpc.get_raw_transaction(&prev_txid, None)?;
    let prev_vout = &prev_tx.output[prev_vout_index as usize];

    let miner_input_address =
        Address::from_script(prev_vout.script_pubkey.as_script(), Network::Regtest)
            .expect("Unable to decode input script to address")
            .to_string();
    let miner_input_amount = prev_vout.value;

    // Get output details
    let mut trader_output_address = String::new();
    let mut trader_output_amount = 0.0;
    let mut miner_change_address = String::new();
    let mut miner_change_amount = 0.0;

    for out in decoded.output.iter() {
        if let Ok(address) = Address::from_script(out.script_pubkey.as_script(), Network::Regtest) {
            let addr_str = address.to_string();

            if addr_str == trader_address.to_string() {
                trader_output_address = addr_str;
                trader_output_amount = out.value.to_btc();
            } else {
                miner_change_address = addr_str;
                miner_change_amount = out.value.to_btc();
            }
        }
    }

    let total_out = ((trader_output_amount + miner_change_amount) * 1e8).round() / 1e8;
    let fee = miner_input_amount - Amount::from_btc(total_out)?;

    // Write the data to ../out.txt in the specified format given in readme.md
    let mut file = File::create("../out.txt")?;
    println!("{txid}");
    writeln!(file, "{txid}")?;
    writeln!(file, "{miner_input_address}")?;
    writeln!(file, "{miner_input_amount}")?;
    writeln!(file, "{trader_output_address}")?;
    writeln!(file, "{trader_output_amount}")?;
    writeln!(file, "{miner_change_address}")?;
    writeln!(file, "{miner_change_amount}")?;
    writeln!(file, "{fee}")?;
    writeln!(file, "{block_height}")?;
    writeln!(file, "{blockhash}")?;

    // Print balances
    let miner_balance = miner_rpc.get_balance(None, None)?;
    let trader_balance = trader_rpc.get_balance(None, None)?;

    println!("\n=== Wallet Balances ===");
    println!("Miner Balance: {:.8} BTC", miner_balance.to_btc());
    println!("Trader Balance: {:.8} BTC", trader_balance.to_btc());

    Ok(())
}
