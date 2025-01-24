// Standard EVM operation costs
pub const SLOAD_COLD: u64 = 2100;
pub const SLOAD_WARM: u64 = 100;
pub const SSTORE_COLD: u64 = 2200;
pub const SSTORE_WARM: u64 = 100;

// Call-related costs
pub const CALL_EMPTY_ACCOUNT: u64 = 25000;
pub const CALL_NEW_ACCOUNT: u64 = 2600;
pub const CALL_VALUE: u64 = 9000;
pub const CALL_BASE: u64 = 100;

// Macro to handle gas accounting for syscalls.
// Returns OutOfGas InterpreterResult if gas limit is exceeded.
#[macro_export]
macro_rules! syscall_gas {
    ($interpreter:expr, $gas_cost:expr $(,)?) => {{
        let gas_cost = $gas_cost;

        trace!("> About to record gas costs:");
        trace!("  - Gas limit: {}", $interpreter.gas.limit());
        trace!("  - Gas prev spent: {}", $interpreter.gas.spent());
        trace!("  - Operation cost: {}", gas_cost);

        if !$interpreter.gas.record_cost(gas_cost) {
            eprintln!("OUT OF GAS");
            return Ok(InterpreterAction::Return {
                result: InterpreterResult {
                    result: InstructionResult::OutOfGas,
                    output: Bytes::new(),
                    gas: $interpreter.gas,
                },
            });
        }

        trace!("> Gas recorded successfully:");
        trace!("  - Gas remaining: {}", $interpreter.gas.remaining());
        trace!("  - Gas spent: {}", $interpreter.gas.spent());
    }};
}
