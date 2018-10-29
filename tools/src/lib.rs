extern crate sr_primitives as runtime_primitives;
extern crate node_runtime as runtime;
extern crate substrate_primitives as primitives;
extern crate srml_balances as balances;
pub extern crate parity_codec;

use runtime_primitives::generic::{UncheckedMortalExtrinsic, Era};
use runtime_primitives::Ed25519Signature;
use runtime::{UncheckedExtrinsic, Address, Runtime, Call};
use primitives::{ed25519, H256};
use parity_codec::Encode;

type Balance = balances::Module<Runtime>;

fn account_id_of(seed: &[u8; 32]) -> Address {
	let account_id: H256 = ed25519::Pair::from_seed(seed).public().0.into();
	Address::from(account_id)
}

fn gen_unchecked_extrinsic(seed: &[u8; 32], index: u64, call: Call, genesis_hash: H256) -> UncheckedExtrinsic {
	let address = account_id_of(seed);
	let payload = (index, call.clone(), Era::Immortal, genesis_hash).encode();
	let pair = ed25519::Pair::from_seed(seed);
	let signature = pair.sign(&payload);
	UncheckedExtrinsic::new_signed(index, call, address, Ed25519Signature(signature), Era::Immortal)
}

#[cfg(tests)]
mod tests {
	use super::*;
	#[test]
	fn test_serialize_extrinsic() {
		let alice_address = account_id_of(b"Alice                           ");
		let bob_address = account_id_of(b"Bob                             ");
		let call = Call::Balances(balances::Call::transfer(alice_address, 12u128));
		let genesis_hash = H256::from_str("");
		let extrinsic = gen_unchecked_extrinsic(b"Alice                           ", 0, call, genesis_hash);
	}
}
