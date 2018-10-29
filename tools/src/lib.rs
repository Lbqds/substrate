extern crate sr_primitives as runtime_primitives;
extern crate node_runtime as runtime;
extern crate substrate_primitives as primitives;
extern crate srml_balances as balances;
extern crate node_primitives;

extern crate parity_codec;
extern crate serde;
extern crate serde_json;

use runtime_primitives::generic::Era;
use runtime_primitives::Ed25519Signature;
use runtime::{UncheckedExtrinsic, Address, Runtime, Call};
use node_primitives::{UncheckedExtrinsic as RawExtrinsic};
use primitives::{ed25519, H256};
use parity_codec::Encode;

// TODO: generic type for AccountId, Balance, Index

fn account_id_of(seed: &[u8; 32]) -> Address {
	let account_id: H256 = ed25519::Pair::from_seed(seed).public().0.into();
	Address::from(account_id)
}

// TODO: support custom transaction era
fn gen_unchecked_extrinsic(seed: &[u8; 32], index: u64, call: Call, genesis_hash: H256) -> UncheckedExtrinsic {
	let address = account_id_of(seed);
	let payload = (index, call.clone(), Era::Immortal, genesis_hash).encode();
	let pair = ed25519::Pair::from_seed(seed);
	let signature = pair.sign(&payload);
	UncheckedExtrinsic::new_signed(index, call, address, Ed25519Signature(signature), Era::Immortal)
}

// Construct transfer transaction which transfer balance from `from_seed` to `to_seed`
fn transfer(from_seed: &[u8; 32], to_seed: &[u8; 32], balance: u64, nonce: u64, genesis_hash: H256) -> UncheckedExtrinsic {
	let from_address = account_id_of(from_seed);
	let to_address = account_id_of(to_seed);
	let call = Call::Balances(balances::Call::transfer(to_address, balance));
	gen_unchecked_extrinsic(from_seed, nonce, call, genesis_hash)
}
