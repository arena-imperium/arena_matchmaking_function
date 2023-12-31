pub use params::*;
use std::str::FromStr;
pub use switchboard_solana::get_ixn_discriminator;
pub use switchboard_solana::prelude::*;

mod params;

#[tokio::main(worker_threads = 12)]
async fn main() {
    // First, initialize the runner instance with a freshly generated Gramine keypair
    let runner = FunctionRunner::new_from_cluster(Cluster::Devnet, None).unwrap();

    // parse and validate user provided request params
    let maybe_params = ContainerParams::decode(
        &runner
            .function_request_data
            .as_ref()
            .unwrap()
            .container_params,
    );

    if maybe_params.is_err() {
        runner.emit_error(1).await.unwrap();
        return;
    }
    let params = maybe_params.unwrap();

    // Generate our random result
    let random_result = generate_randomness(1, 100_000);
    let mut random_bytes = random_result.to_le_bytes().to_vec();

    // IXN DATA:
    // LEN: 13 bytes
    // [0-8]: Anchor Ixn Discriminator
    // [9-12]: Random Result as u32
    // [13]: Faction as u8
    let mut ixn_data = get_ixn_discriminator("arena_matchmaking_settle").to_vec();
    ixn_data.append(&mut random_bytes);
    ixn_data.push(params.faction);

    // ACCOUNTS:
    // 1. Enclave Signer (signer): our Gramine generated keypair
    // 2. User: our user who made the request
    // 3. Realm
    // 4. User Account PDA
    // 5. Spaceship PDA (mut)
    // 6. Switchboard Function (arena_matchmaking_function)
    // 7. Switchboard Function Request
    // 8-9-10-11-12. the spaceships that are potentially being matched with the spaceship_pda
    let settle_ixn = Instruction {
        program_id: params.program_id,
        data: ixn_data,
        accounts: vec![
            AccountMeta::new_readonly(runner.signer, true),
            AccountMeta::new_readonly(params.user, false),
            AccountMeta::new(params.realm_pda, false),
            AccountMeta::new_readonly(params.user_account_pda, false),
            AccountMeta::new(params.spaceship_pda, false),
            AccountMeta::new_readonly(runner.function, false),
            AccountMeta::new_readonly(runner.function_request_key.unwrap(), false),
            AccountMeta::new(params.opponent_spaceship_1_pda, false),
            AccountMeta::new(params.opponent_spaceship_2_pda, false),
            AccountMeta::new(params.opponent_spaceship_3_pda, false),
            AccountMeta::new(params.opponent_spaceship_4_pda, false),
            AccountMeta::new(params.opponent_spaceship_5_pda, false),
        ],
    };

    let increase_compute_budget_ix = Instruction::new_with_borsh(
        solana_sdk::compute_budget::id(),
        &solana_sdk::compute_budget::ComputeBudgetInstruction::SetComputeUnitLimit(1_200_000),
        vec![],
    );

    // Then, write your own Rust logic and build a Vec of instructions.
    // Should  be under 700 bytes after serialization
    let ixs: Vec<solana_program::instruction::Instruction> =
        vec![increase_compute_budget_ix, settle_ixn];

    // Finally, emit the signed quote and partially signed transaction to the functionRunner oracle
    // The functionRunner oracle will use the last outputted word to stdout as the serialized result. This is what gets executed on-chain.
    match runner.emit(ixs).await {
        Ok(_) => (),
        Err(_error) => {
            let _ = runner.emit_error(3).await;
            return;
        }
    };
}

fn generate_randomness(min: u32, max: u32) -> u32 {
    if min == max {
        return min;
    }
    if min > max {
        return generate_randomness(max, min);
    }

    // We add one so its inclusive [min, max]
    let window = (max + 1) - min;

    let mut bytes: [u8; 4] = [0u8; 4];
    Gramine::read_rand(&mut bytes).expect("gramine failed to generate randomness");
    let raw_result: &[u32] = bytemuck::cast_slice(&bytes[..]);

    (raw_result[0] % window) + min
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1. Check when lower_bound is greater than upper_bound
    #[test]
    fn test_generate_randomness_with_flipped_bounds() {
        let min = 100;
        let max = 50;

        let result = generate_randomness(100, 50);
        assert!(result >= max && result < min);
    }

    // 2. Check when lower_bound is equal to upper_bound
    #[test]
    fn test_generate_randomness_with_equal_bounds() {
        let bound = 100;
        assert_eq!(generate_randomness(bound, bound), bound);
    }

    // 3. Test within a range
    #[test]
    fn test_generate_randomness_within_bounds() {
        let min = 100;
        let max = 200;

        let result = generate_randomness(min, max);

        assert!(result >= min && result < max);
    }

    // 4. Test randomness distribution (not truly deterministic, but a sanity check)
    #[test]
    fn test_generate_randomness_distribution() {
        let min = 0;
        let max = 9;

        let mut counts = vec![0; 10];
        for _ in 0..1000 {
            let result = generate_randomness(min, max);
            let index: usize = result as usize;
            counts[index] += 1;
        }

        // Ensure all counts are non-zero (probabilistically should be the case)
        for count in counts.iter() {
            assert!(*count > 0);
        }
    }

    #[test]
    fn test_generate_randomness_and_encode() {
        let faction = 1u8;
        let min = 0;
        let max = 10000;

        let result = generate_randomness(min, max);
        let mut random_bytes = result.to_le_bytes().to_vec();

        let mut ixn_data = get_ixn_discriminator("arena_matchmaking_settle").to_vec();
        ixn_data.append(&mut random_bytes);
        ixn_data.push(faction);
        // ixn_data.append(&mut faction.to_le_bytes().to_vec());
    }
}
