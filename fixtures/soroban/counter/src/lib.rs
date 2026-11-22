//! Minimal counter contract used as a genuine Soroban-built test artifact.
//!
//! The implementation is deliberately small; the value of this fixture is the
//! specification the Soroban SDK emits for it, not the counter logic.

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CounterState {
    pub value: u32,
    pub owner: soroban_sdk::Address,
}

#[contract]
pub struct Counter;

#[contractimpl]
impl Counter {
    pub fn increment(env: Env) -> u32 {
        let key = Symbol::new(&env, "value");
        let value: u32 = env.storage().instance().get(&key).unwrap_or(0) + 1;
        env.storage().instance().set(&key, &value);
        value
    }

    pub fn value(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "value"))
            .unwrap_or(0)
    }

    pub fn state(env: Env) -> CounterState {
        let owner = env.current_contract_address();
        CounterState {
            value: Self::value(env),
            owner,
        }
    }
}
