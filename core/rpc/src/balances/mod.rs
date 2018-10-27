use std::sync::Arc;

use jsonrpc_macros::Trailing;
use client::{self, Client};
use runtime_primitives::traits::{Block as BlockT};
use primitives::{H256, Blake2Hasher};

mod error;

use self::error::Result;

build_rpc_trait! {
    // Substrate balances API
	pub trait BalancesApi {
		// Get free balance of account
		#[rpc(name = "balances_free")]
		fn free_balance_of(&self, Trailing<H256>) -> Result<u64>;

		// Get reserved balance of account
		#[rpc(name = "balances_reserved")]
		fn reserved_balance_of(&self, Trailing<H256>) -> Result<u64>;
	}
}

// Substrate balances API
pub struct Balances<B, E, Block: BlockT> {
	client: Arc<Client<B, E, Block>>,
}

impl<B, E, Block: BlockT> Balances<B, E, Block> {
	// Create new balances API handler
	pub fn new(client: Arc<Client<B, E, Block>>) -> Self {
		Self {
			client,
		}
	}
}

impl<B, E, Block> BalancesApi for Balances<B, E, Block> where
	Block: BlockT + 'static,
	B: client::backend::Backend<Block, Blake2Hasher> + Send + Sync + 'static,
	E: client::CallExecutor<Block, Blake2Hasher> + Send + Sync + 'static,
{
	fn free_balance_of(&self, acc: Trailing<H256>) -> Result<u64> {
		let account: Option<H256> = acc.into();
		let account_id = account.unwrap_or(Default::default());
		let balance = self.client.call_api::<H256, u64>("free_balance_of", &account_id)?;
		Ok(balance)
	}

	fn reserved_balance_of(&self, acc: Trailing<H256>) -> Result<u64> {
		let account: Option<H256> = acc.into();
		let account_id = account.unwrap_or(Default::default());
		let balance = self.client.call_api::<H256, u64>("reserved_balance_of", &account_id)?;
		Ok(balance)
	}
}
