use alloy_core::primitives::keccak256;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ImplItemMethod, ReturnType, TraitItemMethod};

// Unified method info from `ImplItemMethod` and `TraitItemMethod`
pub struct MethodInfo<'a> {
    name: &'a Ident,
    args: Vec<syn::FnArg>,
    return_type: &'a ReturnType,
}

impl<'a> From<&'a ImplItemMethod> for MethodInfo<'a> {
    fn from(method: &'a ImplItemMethod) -> Self {
        Self {
            name: &method.sig.ident,
            args: method.sig.inputs.iter().cloned().collect(),
            return_type: &method.sig.output,
        }
    }
}

impl<'a> From<&'a TraitItemMethod> for MethodInfo<'a> {
    fn from(method: &'a TraitItemMethod) -> Self {
        Self {
            name: &method.sig.ident,
            args: method.sig.inputs.iter().cloned().collect(),
            return_type: &method.sig.output,
        }
    }
}

// Helper function to get the parameter names + types of a method
fn get_arg_props<'a>(
    skip_first_arg: bool,
    method: &'a MethodInfo<'a>,
) -> (Vec<Ident>, Vec<&syn::Type>) {
    method
        .args
        .iter()
        .skip(if skip_first_arg { 1 } else { 0 })
        .enumerate()
        .map(|(i, arg)| {
            if let FnArg::Typed(pat_type) = arg {
                (format_ident!("arg{}", i), &*pat_type.ty)
            } else {
                panic!("Expected typed arguments");
            }
        })
        .unzip()
}

pub fn get_arg_props_skip_first<'a>(method: &'a MethodInfo<'a>) -> (Vec<Ident>, Vec<&syn::Type>) {
    get_arg_props(true, method)
}

pub fn get_arg_props_all<'a>(method: &'a MethodInfo<'a>) -> (Vec<Ident>, Vec<&syn::Type>) {
    get_arg_props(false, method)
}

// Helper function to generate interface impl from user-defined methods
pub fn generate_interface<T>(
    methods: &[&T],
    interface_name: &Ident,
) -> quote::__private::TokenStream
where
    for<'a> MethodInfo<'a>: From<&'a T>,
{
    let methods: Vec<MethodInfo> = methods.iter().map(|&m| MethodInfo::from(m)).collect();

    // Generate implementation
    let method_impls = methods.iter().map(|method| {
        let name = method.name;
        let return_type = method.return_type;
        let method_selector = u32::from_be_bytes(
            keccak256(name.to_string())[..4]
                .try_into()
                .unwrap_or_default(),
        );

        let (arg_names, arg_types) = get_arg_props_skip_first(method);

        let calldata = if arg_names.is_empty() {
            quote! {
                let mut complete_calldata = Vec::with_capacity(4);
                complete_calldata.extend_from_slice(&[
                    #method_selector.to_be_bytes()[0],
                    #method_selector.to_be_bytes()[1],
                    #method_selector.to_be_bytes()[2],
                    #method_selector.to_be_bytes()[3],
                ]);
            }
        } else {
            quote! {
                let mut args_calldata = (#(#arg_names),*).abi_encode();
                let mut complete_calldata = Vec::with_capacity(4 + args_calldata.len());
                complete_calldata.extend_from_slice(&[
                    #method_selector.to_be_bytes()[0],
                    #method_selector.to_be_bytes()[1],
                    #method_selector.to_be_bytes()[2],
                    #method_selector.to_be_bytes()[3],
                ]);
                complete_calldata.append(&mut args_calldata);
            }
        };

        let return_ty = match return_type {
            ReturnType::Default => quote! { () },
            ReturnType::Type(_, ty) => quote! { #ty },
        };

        quote! {
            pub fn #name(&self, #(#arg_names: #arg_types),*) -> Option<#return_ty> {
                use alloy_sol_types::SolValue;
                use alloc::vec::Vec;

                #calldata

                let result = eth_riscv_runtime::call_contract(
                    self.address,
                    0_u64,
                    &complete_calldata,
                    32_u64
                )?;

                <#return_ty>::abi_decode(&result, true).ok()
            }
        }
    });

    quote! {
        pub struct #interface_name {
            address: Address,
        }

        impl #interface_name {
            pub fn new(address: Address) -> Self {
                Self { address }
            }

            #(#method_impls)*
        }
    }
}

// Helper function to generate the deployment code
pub fn generate_deployment_code(
    _struct_name: &Ident,
    constructor: Option<&ImplItemMethod>,
) -> quote::__private::TokenStream {
    quote! {
        use alloc::vec::Vec;

        #[no_mangle]
        pub extern "C" fn main() -> ! {
            // TODO: figure out constructor

            let runtime: &[u8] = include_bytes!("../target/riscv64imac-unknown-none-elf/release/runtime");
            let mut prepended_runtime = Vec::with_capacity(1 + runtime.len());
            prepended_runtime.push(0xff);
            prepended_runtime.extend_from_slice(runtime);

            let prepended_runtime_slice: &[u8] = &prepended_runtime;
            let result_ptr = prepended_runtime_slice.as_ptr() as u64;
            let result_len = prepended_runtime_slice.len() as u64;
            eth_riscv_runtime::return_riscv(result_ptr, result_len);
        }
    }
}
