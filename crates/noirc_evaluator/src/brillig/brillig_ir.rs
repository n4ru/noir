//! This module is an abstraction layer over `Brillig`
//! To allow for separation of concerns, it knows nothing
//! about SSA types, and can therefore be tested independently.
//! `brillig_gen` is therefore the module which combines both
//! ssa types and types in this module.
//! A similar paradigm can be seen with the `acir_ir` module.
pub(crate) mod artifact;
pub(crate) mod memory;

use self::{
    artifact::{BrilligArtifact, UnresolvedJumpLocation},
    memory::BrilligMemory,
};
use acvm::{
    acir::brillig_vm::{
        BinaryFieldOp, BinaryIntOp, Opcode as BrilligOpcode, RegisterIndex, RegisterValueOrArray,
        Value,
    },
    FieldElement,
};

/// Brillig context object that is used while constructing the
/// Brillig bytecode.
#[derive(Default)]
pub(crate) struct BrilligContext {
    obj: BrilligArtifact,
    /// A usize indicating the latest un-used register.
    latest_register: usize,
    /// Tracks memory allocations
    memory: BrilligMemory,
}

impl BrilligContext {
    /// Adds a brillig instruction to the brillig byte code
    pub(crate) fn push_opcode(&mut self, opcode: BrilligOpcode) {
        self.obj.byte_code.push(opcode);
    }

    /// Returns the artifact
    pub(crate) fn artifact(self) -> BrilligArtifact {
        self.obj
    }

    /// Allocates an array of size `size` and stores the pointer to the array
    /// in `pointer_register`
    pub(crate) fn allocate_array(
        &mut self,
        pointer_register: RegisterIndex,
        size: u32,
        prefilled: bool,
    ) {
        let allocation = self.memory.allocate(size as usize);

        // If the array is prefilled (for example, parameter arrays), then we do not need to expand memory
        if !prefilled {
            // Create a new register to store the pointer to the memory address
            // of the last element in the array
            let end_memory_address = self.create_register();
            self.const_instruction(end_memory_address, allocation.end_address.into());
            // Emit a store instruction for the last element in the array.
            // The VM will expand the memory and zero fill all of the elements
            // from `start_address` to `end_address`
            //
            // Since it is a store instruction, we need to indicate a value to
            // store at `end_address` in memory. We will store zero here.
            let zero = self.create_register();
            self.const_instruction(zero, Value::from(0u128));
            self.store_instruction(end_memory_address, zero);
        }

        self.push_opcode(BrilligOpcode::Const {
            destination: pointer_register,
            value: Value::from(allocation.start_address),
        });
    }

    /// Gets the value in the array at index `index` and stores it in `result`
    pub(crate) fn array_get(
        &mut self,
        array_ptr: RegisterIndex,
        index: RegisterIndex,
        result: RegisterIndex,
    ) {
        // Computes array_ptr + index, ie array[index]
        let index_of_element_in_memory = self.create_register();
        self.binary_instruction(
            array_ptr,
            index,
            index_of_element_in_memory,
            BrilligBinaryOp::Field { op: BinaryFieldOp::Add },
        );

        self.load_instruction(result, index_of_element_in_memory);
    }

    /// Stores the value in the array at index `index`
    pub(crate) fn array_store(
        &mut self,
        array_ptr: RegisterIndex,
        index: RegisterIndex,
        value: RegisterIndex,
    ) {
        // Computes array_ptr + index, ie array[index]
        let index_of_element_in_memory = self.create_register();
        self.binary_instruction(
            array_ptr,
            index,
            index_of_element_in_memory,
            BrilligBinaryOp::Field { op: BinaryFieldOp::Add },
        );

        self.store_instruction(index_of_element_in_memory, value);
    }

    /// Adds a label to the next opcode
    pub(crate) fn add_label_to_next_opcode<T: ToString>(&mut self, label: T) {
        self.obj.add_label_at_position(label.to_string(), self.obj.index_of_next_opcode());
    }

    /// Adds a unresolved `Jump` instruction to the bytecode.
    pub(crate) fn jump_instruction<T: ToString>(&mut self, target_label: T) {
        self.add_unresolved_jump(
            BrilligOpcode::Jump { location: 0 },
            UnresolvedJumpLocation::Label(target_label.to_string()),
        );
    }

    /// Adds a unresolved `JumpIf` instruction to the bytecode.
    pub(crate) fn jump_if_instruction<T: ToString>(
        &mut self,
        condition: RegisterIndex,
        target_label: T,
    ) {
        self.add_unresolved_jump(
            BrilligOpcode::JumpIf { condition, location: 0 },
            UnresolvedJumpLocation::Label(target_label.to_string()),
        );
    }

    /// Adds a unresolved `Jump` instruction to the bytecode.
    fn add_unresolved_jump(
        &mut self,
        jmp_instruction: BrilligOpcode,
        destination: UnresolvedJumpLocation,
    ) {
        self.obj.add_unresolved_jump(jmp_instruction, destination);
    }

    /// Creates a new register.
    pub(crate) fn create_register(&mut self) -> RegisterIndex {
        let register = RegisterIndex::from(self.latest_register);

        // Note: We could insert a const instruction to initialize the register
        // because the VM will not expand the register space automatically.
        //
        // In most cases, the register created is used in another instruction
        // which will cause the VM to expand the register space, but this is not
        // a guarantee.
        //
        // This would only be possible if it's not a function parameter register.
        //
        // TODO: check if the above can be true if we just return a constant for example
        // TODO from a program
        // self.const_instruction(register, 0u128.into());

        self.latest_register += 1;
        register
    }
}

impl BrilligContext {
    /// Emits brillig bytecode to jump to a trap condition if `condition`
    /// is false.
    pub(crate) fn constrain_instruction(&mut self, condition: RegisterIndex) {
        // Jump to the relative location after the trap
        self.add_unresolved_jump(
            BrilligOpcode::JumpIf { condition, location: 0 },
            UnresolvedJumpLocation::Relative(2),
        );
        self.push_opcode(BrilligOpcode::Trap);
    }

    /// Processes a return instruction.
    ///
    /// For Brillig, the return is implicit, since there is no explicit return instruction.
    /// The caller will take `N` values from the Register starting at register index 0.
    /// `N` indicates the number of return values expected.
    ///
    /// Brillig does not have an explicit return instruction, so this
    /// method will move all register values to the first `N` values in
    /// the VM.
    pub(crate) fn return_instruction(&mut self, return_registers: &[RegisterIndex]) {
        for (destination_index, return_register) in return_registers.iter().enumerate() {
            // If the destination register index is more than the latest register,
            // we update the latest register to be the destination register because the
            // brillig vm will expand the number of registers internally, when it encounters
            // a register that has not been initialized.
            if destination_index > self.latest_register {
                self.latest_register = destination_index;
            }
            self.mov_instruction(destination_index.into(), *return_register);
        }
        self.stop_instruction();
    }

    /// Emits a `mov` instruction.
    ///
    /// Copies the value at `source` into `destination`
    pub(crate) fn mov_instruction(&mut self, destination: RegisterIndex, source: RegisterIndex) {
        self.push_opcode(BrilligOpcode::Mov { destination, source });
    }

    /// Processes a binary instruction according `operation`.
    ///
    /// This method will compute lhs <operation> rhs
    /// and store the result in the `result` register.
    pub(crate) fn binary_instruction(
        &mut self,
        lhs: RegisterIndex,
        rhs: RegisterIndex,
        result: RegisterIndex,
        operation: BrilligBinaryOp,
    ) {
        match operation {
            BrilligBinaryOp::Field { op } => {
                let opcode = BrilligOpcode::BinaryFieldOp { op, destination: result, lhs, rhs };
                self.push_opcode(opcode);
            }
            BrilligBinaryOp::Integer { op, bit_size } => {
                let opcode =
                    BrilligOpcode::BinaryIntOp { op, destination: result, bit_size, lhs, rhs };
                self.push_opcode(opcode);
            }
            BrilligBinaryOp::Modulo { is_signed_integer, bit_size } => {
                self.modulo_instruction(result, lhs, rhs, bit_size, is_signed_integer);
            }
        }
    }

    /// Generates the instructions to apply a binary operation to all items of two arrays.
    pub(crate) fn arrays_binary_instruction(
        &mut self,
        lhs_array_ptr: RegisterIndex,
        rhs_array_ptr: RegisterIndex,
        result_array_ptr: RegisterIndex,
        num_elements: u32,
        binary_operation: BrilligBinaryOp,
    ) {
        // Reserve a register for the result of each comparation
        let index_comparison_register = self.create_register();

        // Reserve a register for the index being compared
        let index_register = self.create_register();

        // Reserve registers for the values of left and right
        let left_value_register = self.create_register();
        let right_value_register = self.create_register();

        for i in 0..num_elements {
            // Load both values
            self.const_instruction(index_register, (i as u128).into());
            self.array_get(lhs_array_ptr, index_register, left_value_register);
            self.const_instruction(index_register, (i as u128).into());
            self.array_get(rhs_array_ptr, index_register, right_value_register);

            // Compare the values
            self.binary_instruction(
                left_value_register,
                right_value_register,
                index_comparison_register,
                binary_operation,
            );
            self.const_instruction(index_register, (i as u128).into());
            self.array_store(result_array_ptr, index_register, index_comparison_register);
        }
    }

    pub(crate) fn array_reduce(
        &mut self,
        array_ptr: RegisterIndex,
        result_register: RegisterIndex,
        num_elements: u32,
        reduce_operation: BrilligBinaryOp,
    ) {
        // Reserve a register for the index being compared
        let index_register = self.create_register();

        // Reserve register for the value at the index
        let value_register = self.create_register();

        for i in 0..num_elements {
            // Load value
            self.const_instruction(index_register, (i as u128).into());
            self.array_get(array_ptr, index_register, value_register);

            // Reduce the value
            self.binary_instruction(
                result_register,
                value_register,
                result_register,
                reduce_operation,
            );
        }
    }

    /// Stores the value of `constant` in the `result` register
    pub(crate) fn const_instruction(&mut self, result: RegisterIndex, constant: Value) {
        self.push_opcode(BrilligOpcode::Const { destination: result, value: constant });
    }

    /// Processes a not instruction.
    ///
    /// Not is computed using a subtraction operation as there is no native not instruction
    /// in Brillig.
    pub(crate) fn not_instruction(&mut self, condition: RegisterIndex, result: RegisterIndex) {
        let one = self.make_constant(Value::from(FieldElement::one()));

        // Compile !x as (1 - x)
        let opcode = BrilligOpcode::BinaryIntOp {
            destination: result,
            op: BinaryIntOp::Sub,
            bit_size: 1,
            lhs: one,
            rhs: condition,
        };
        self.push_opcode(opcode);
    }

    /// Processes a foreign call instruction.
    ///
    /// Note: the function being called is external and will
    /// not be linked during brillig generation.
    pub(crate) fn foreign_call_instruction(
        &mut self,
        func_name: String,
        inputs: &[RegisterValueOrArray],
        outputs: &[RegisterValueOrArray],
    ) {
        // TODO(https://github.com/noir-lang/acvm/issues/366): Enable multiple inputs and outputs to a foreign call
        let opcode = BrilligOpcode::ForeignCall {
            function: func_name,
            destination: outputs[0],
            input: inputs[0],
        };
        self.push_opcode(opcode);
    }

    /// Emits a load instruction
    pub(crate) fn load_instruction(
        &mut self,
        destination: RegisterIndex,
        source_pointer: RegisterIndex,
    ) {
        self.push_opcode(BrilligOpcode::Load { destination, source_pointer });
    }

    /// Emits a store instruction
    pub(crate) fn store_instruction(
        &mut self,
        destination_pointer: RegisterIndex,
        source: RegisterIndex,
    ) {
        self.push_opcode(BrilligOpcode::Store { destination_pointer, source });
    }

    /// Emits a truncate instruction.
    ///
    /// Note: Truncation is used as an optimization in the SSA IR
    /// for the ACIR generation pass; ACIR gen does not overflow
    /// on every integer operation since it would be in-efficient.
    /// Instead truncation instructions are emitted as to when a
    /// truncation should be done.
    /// For Brillig, all integer operations will overflow as its cheap.
    pub(crate) fn truncate_instruction(
        &mut self,
        destination_of_truncated_value: RegisterIndex,
        value_to_truncate: RegisterIndex,
    ) {
        // Effectively a no-op because brillig already has implicit truncation on integer
        // operations. We need only copy the value to it's destination.
        self.mov_instruction(destination_of_truncated_value, value_to_truncate);
    }

    /// Emits a stop instruction
    pub(crate) fn stop_instruction(&mut self) {
        self.push_opcode(BrilligOpcode::Stop);
    }

    /// Returns a register which holds the value of a constant
    pub(crate) fn make_constant(&mut self, constant: Value) -> RegisterIndex {
        let register = self.create_register();
        self.const_instruction(register, constant);
        register
    }

    /// Computes left % right by emitting the necessary Brillig opcodes.
    ///
    /// This is done by using the following formula:
    ///
    /// a % b = a - (b * (a / b))
    ///
    /// Brillig does not have an explicit modulo operation,
    /// so we must emit multiple opcodes and process it differently
    /// to other binary instructions.
    pub(crate) fn modulo_instruction(
        &mut self,
        result_register: RegisterIndex,
        left: RegisterIndex,
        right: RegisterIndex,
        bit_size: u32,
        signed: bool,
    ) {
        let scratch_register_i = self.create_register();
        let scratch_register_j = self.create_register();

        // i = left / right
        self.push_opcode(BrilligOpcode::BinaryIntOp {
            op: match signed {
                true => BinaryIntOp::SignedDiv,
                false => BinaryIntOp::UnsignedDiv,
            },
            destination: scratch_register_i,
            bit_size,
            lhs: left,
            rhs: right,
        });

        // j = i * right
        self.push_opcode(BrilligOpcode::BinaryIntOp {
            op: BinaryIntOp::Mul,
            destination: scratch_register_j,
            bit_size,
            lhs: scratch_register_i,
            rhs: right,
        });

        // result_register = left - j
        self.push_opcode(BrilligOpcode::BinaryIntOp {
            op: BinaryIntOp::Sub,
            destination: result_register,
            bit_size,
            lhs: left,
            rhs: scratch_register_j,
        });
    }
}

#[derive(Debug, Clone, Copy)]
/// Type to encapsulate the binary operation types in Brillig
pub(crate) enum BrilligBinaryOp {
    Field { op: BinaryFieldOp },
    Integer { op: BinaryIntOp, bit_size: u32 },
    // Modulo operation requires more than one opcode
    // Brillig.
    Modulo { is_signed_integer: bool, bit_size: u32 },
}