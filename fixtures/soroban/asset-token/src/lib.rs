//! Small token interface used as a genuine Soroban-built test artifact.
//!
//! The implementation is deliberately minimal; the value of this fixture is
//! the specification the Soroban SDK emits for it, not token semantics.

#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenMetadata {
    pub name: Symbol,
    pub symbol: Symbol,
    pub decimals: u32,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TokenStatus {
    Active = 0,
    Frozen = 1,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TokenError {
    InsufficientBalance = 1,
    Unauthorized = 2,
}

#[contract]
pub struct AssetToken;

#[contractimpl]
impl AssetToken {
    pub fn initialize(env: Env, admin: Address, metadata: TokenMetadata) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "admin"), &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "metadata"), &metadata);
    }

    pub fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "admin"))
            .unwrap()
    }

    pub fn metadata(env: Env) -> TokenMetadata {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "metadata"))
            .unwrap()
    }

    pub fn balance(_env: Env, _owner: Address) -> i128 {
        0
    }

    pub fn transfer(
        _env: Env,
        _from: Address,
        _to: Address,
        _amount: i128,
    ) -> Result<(), TokenError> {
        Ok(())
    }

    pub fn set_status(_env: Env, status: TokenStatus) -> TokenStatus {
        status
    }
}
