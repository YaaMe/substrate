// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate runtime interface
//!
//! This crate provides types, traits and macros around runtime interfaces. A runtime interface is
//! a fixed interface between a Substrate runtime and a Substrate node. For a native runtime the
//! interface maps to a direct function call of the implementation. For a wasm runtime the interface
//! maps to an external function call. These external functions are exported by the wasm executor
//! and they map to the same implementation as the native calls.
//!
//! # Using a type in a runtime interface
//!
//! Any type that should be used in a runtime interface as argument or return value needs to
//! implement [`RIType`]. The associated type `FFIType` is the type that is used in the FFI
//! function to represent the actual type. For example `[T]` is represented by an `u64`. The slice
//! pointer and the length will be mapped to an `u64` value. For more information, see the
//! implementation of [`RIType`] for [`T`]. The FFI function definition is used when calling from
//! the wasm runtime into the node.
//!
//! Traits are used to convert from a type to the corresponding [`RIType::FFIType`].
//! Depending on where and how a type should be used in a function signature, a combination of the
//! following traits need to be implemented:
//!
//! 1. Pass as function argument: [`wasm::IntoFFIValue`] and [`host::FromFFIValue`]
//! 2. As function return value: [`wasm::FromFFIValue`] and [`host::IntoFFIValue`]
//! 3. Pass as mutable function argument: [`host::IntoPreallocatedFFIValue`]
//!
//! The traits are implemented for most of the common types like `[T]`, `Vec<T>`, arrays and
//! primitive types.
//!
//! For custom types, we provide the [`PassBy`](pass_by::PassBy) trait and strategies that define
//! how a type is passed between the wasm runtime and the node. Each strategy also provides a derive
//! macro to simplify the implementation.
//!
//! # Performance
//!
//! To not waste any more performance when calling into the node, not all types are SCALE encoded
//! when being passed as arguments between the wasm runtime and the node. For most types that
//! are raw bytes like `Vec<u8>`, `[u8]` or `[u8; N]` we pass them directly, without SCALE encoding
//! them in front of. The implementation of [`RIType`] each type provides more information on how
//! the data is passed.
//!
//! # Declaring a runtime interface
//!
//! Declaring a runtime interface is similar to declaring a trait in Rust:
//!
//! ```
//! #[sp_runtime_interface::runtime_interface]
//! trait RuntimeInterface {
//!     fn some_function(value: &[u8]) -> bool {
//!         value.iter().all(|v| *v > 125)
//!     }
//! }
//! ```
//!
//! For more information on declaring a runtime interface, see
//! [`#[runtime_interface]`](attr.runtime_interface.html).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate self as sp_runtime_interface;

#[doc(hidden)]
#[cfg(feature = "std")]
pub use wasm_interface;

#[doc(hidden)]
pub use sp_std;

/// Attribute macro for transforming a trait declaration into a runtime interface.
///
/// A runtime interface is a fixed interface between a Substrate compatible runtime and the native
/// node. This interface is callable from a native and a wasm runtime. The macro will generate the
/// corresponding code for the native implementation and the code for calling from the wasm
/// side to the native implementation.
///
/// The macro expects the runtime interface declaration as trait declaration:
///
/// ```
/// # use sp_runtime_interface::runtime_interface;
///
/// #[runtime_interface]
/// trait Interface {
///     /// A function that can be called from native/wasm.
///     ///
///     /// The implementation given to this function is only compiled on native.
///     fn call_some_complex_code(data: &[u8]) -> Vec<u8> {
///         // Here you could call some rather complex code that only compiles on native or
///         // is way faster in native than executing it in wasm.
///         Vec::new()
///     }
///
///     /// A function can take a `&self` or `&mut self` argument to get access to the
///     /// `Externalities`. (The generated method does not require
///     /// this argument, so the function can be called just with the `optional` argument)
///     fn set_or_clear(&mut self, optional: Option<Vec<u8>>) {
///         match optional {
///             Some(value) => self.set_storage([1, 2, 3, 4].to_vec(), value),
///             None => self.clear_storage(&[1, 2, 3, 4]),
///         }
///     }
/// }
/// ```
///
///
/// The given example will generate roughly the following code for native:
///
/// ```
/// // The name of the trait is converted to snake case and used as mod name.
/// //
/// // Be aware that this module is not `public`, the visibility of the module is determined based
/// // on the visibility of the trait declaration.
/// mod interface {
///     trait Interface {
///         fn call_some_complex_code(data: &[u8]) -> Vec<u8>;
///         fn set_or_clear(&mut self, optional: Option<Vec<u8>>);
///     }
///
///     impl Interface for &mut dyn externalities::Externalities {
///         fn call_some_complex_code(data: &[u8]) -> Vec<u8> { Vec::new() }
///         fn set_or_clear(&mut self, optional: Option<Vec<u8>>) {
///             match optional {
///                 Some(value) => self.set_storage([1, 2, 3, 4].to_vec(), value),
///                 None => self.clear_storage(&[1, 2, 3, 4]),
///             }
///         }
///     }
///
///     pub fn call_some_complex_code(data: &[u8]) -> Vec<u8> {
///         <&mut dyn externalities::Externalities as Interface>::call_some_complex_code(data)
///     }
///
///     pub fn set_or_clear(optional: Option<Vec<u8>>) {
///         externalities::with_externalities(|mut ext| Interface::set_or_clear(&mut ext, optional))
///             .expect("`set_or_clear` called outside of an Externalities-provided environment.")
///     }
///
///     /// This type implements the `HostFunctions` trait (from `sp-wasm-interface`) and
///     /// provides the host implementation for the wasm side. The host implementation converts the
///     /// arguments from wasm to native and calls the corresponding native function.
///     ///
///     /// This type needs to be passed to the wasm executor, so that the host functions will be
///     /// registered in the executor.
///     pub struct HostFunctions;
/// }
/// ```
///
///
/// The given example will generate roughly the following code for wasm:
///
/// ```
/// mod interface {
///     mod extern_host_functions_impls {
///         extern "C" {
///             /// Every function is exported as `ext_TRAIT_NAME_FUNCTION_NAME_version_VERSION`.
///             ///
///             /// `TRAIT_NAME` is converted into snake case.
///             ///
///             /// The type for each argument of the exported function depends on
///             /// `<ARGUMENT_TYPE as RIType>::FFIType`.
///             ///
///             /// `data` holds the pointer and the length to the `[u8]` slice.
///             pub fn ext_Interface_call_some_complex_code_version_1(data: u64) -> u64;
///             /// `optional` holds the pointer and the length of the encoded value.
///             pub fn ext_Interface_set_or_clear_version_1(optional: u64);
///         }
///     }
///
///     /// The type is actually `ExchangeableFunction` (from `sp-runtime-interface`).
///     ///
///     /// This can be used to replace the implementation of the `call_some_complex_code` function.
///     /// Instead of calling into the host, the callee will automatically call the other
///     /// implementation.
///     ///
///     /// To replace the implementation:
///     ///
///     /// `host_call_some_complex_code.replace_implementation(some_other_impl)`
///     pub static host_call_some_complex_code: () = ();
///     pub static host_set_or_clear: () = ();
///
///     pub fn call_some_complex_code(data: &[u8]) -> Vec<u8> {
///         // This is the actual call: `host_call_some_complex_code.get()(data)`
///         //
///         // But that does not work for several reasons in this example, so we just return an
///         // empty vector.
///         Vec::new()
///     }
///
///     pub fn set_or_clear(optional: Option<Vec<u8>>) {
///         // Same as above
///     }
/// }
/// ```
///
/// # Argument types
///
/// The macro supports any kind of argument type, as long as it implements [`RIType`] and the
/// required `FromFFIValue`/`IntoFFIValue`. The macro will convert each
/// argument to the corresponding FFI representation and will call into the host using this FFI
/// representation. On the host each argument is converted back to the native representation and
/// the native implementation is called. Any return value is handled in the same way.
///
/// # Wasm only interfaces
///
/// Some interfaces are only required from within the wasm runtime e.g. the allocator interface.
/// To support this, the macro can be called like `#[runtime_interface(wasm_only)]`. This instructs
/// the macro to make two significant changes to the generated code:
///
/// 1. The generated functions are not callable from the native side.
/// 2. The trait as shown above is not implemented for `Externalities` and is instead implemented
///    for `FunctionExecutor` (from `sp-wasm-interface`).
pub use sp_runtime_interface_proc_macro::runtime_interface;

#[doc(hidden)]
#[cfg(feature = "std")]
pub use externalities::{
	set_and_run_with_externalities, with_externalities, Externalities, ExternalitiesExt, ExtensionStore,
};

#[doc(hidden)]
pub use codec;

pub(crate) mod impls;
#[cfg(feature = "std")]
pub mod host;
#[cfg(not(feature = "std"))]
pub mod wasm;
pub mod pass_by;

/// Something that can be used by the runtime interface as type to communicate between wasm and the
/// host.
///
/// Every type that should be used in a runtime interface function signature needs to implement
/// this trait.
pub trait RIType {
	/// The ffi type that is used to represent `Self`.
	#[cfg(feature = "std")]
	type FFIType: wasm_interface::IntoValue + wasm_interface::TryFromValue;
	#[cfg(not(feature = "std"))]
	type FFIType;
}

/// A pointer that can be used in a runtime interface function signature.
#[cfg(not(feature = "std"))]
pub type Pointer<T> = *mut T;

/// A pointer that can be used in a runtime interface function signature.
#[cfg(feature = "std")]
pub type Pointer<T> = wasm_interface::Pointer<T>;