use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_attribute]
pub fn distilled(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _attr = syn::parse_macro_input!(attr as syn::AttributeArgs);
    let item = syn::parse_macro_input!(item as syn::ItemFn);
    let args = item.sig.inputs;
    let pats = syn::PatTuple {
        attrs: vec![],
        paren_token: syn::token::Paren::default(),
        elems: args
            .iter()
            .map(|a| match a {
                syn::FnArg::Typed(t) => *t.pat.clone(),
                _ => unimplemented!(),
            })
            .collect(),
    };
    let tys = syn::TypeTuple {
        paren_token: syn::token::Paren::default(),
        elems: args
            .iter()
            .map(|a| match a {
                syn::FnArg::Typed(t) => *t.ty.clone(),
                _ => unimplemented!(),
            })
            .collect(),
    };
    let ret = item.sig.output;
    let ret_type = match &ret {
        syn::ReturnType::Default => syn::Type::Tuple(syn::TypeTuple {
            paren_token: syn::token::Paren::default(),
            elems: syn::punctuated::Punctuated::new(),
        }),
        syn::ReturnType::Type(_, t) => *t.clone(),
    };
    let body = item.block;
    let fn_name = item.sig.ident;
    let mod_name = syn::Ident::new(&format!("_distilled_mod_{}", fn_name), fn_name.span());
    let wrapper_name = syn::Ident::new(&format!("_distilled_wrapper_{}", fn_name), fn_name.span());
    let wrapper_name_str = wrapper_name.to_string();
    let get_in_name = syn::Ident::new(&format!("_distilled_get_in_{}", fn_name), fn_name.span());
    let get_in_name_str = get_in_name.to_string();
    let get_out_name = syn::Ident::new(&format!("_distilled_get_out_{}", fn_name), fn_name.span());
    let get_out_name_str = get_out_name.to_string();

    TokenStream::from(quote! {
        #[cfg(not(target_arch = "wasm32"))]
        pub fn #fn_name() -> ::distilled::iter::WasmFn<(#tys), #ret_type> {
            ::distilled::iter::WasmFn {
                entry: #wrapper_name_str.to_string(),
                get_in: #get_in_name_str.to_string(),
                get_out: #get_out_name_str.to_string(),
                _phantom: ::std::marker::PhantomData,
            }
        }

        #[cfg(target_arch = "wasm32")]
        mod #mod_name {
            use ::nanoserde::{DeBin, SerBin};
            const IN_BUFFER_SIZE: usize = 1024;
            static mut IN_BUFFER: &[u8] = &[0; IN_BUFFER_SIZE];
            const OUT_BUFFER_SIZE: usize = 1024;
            static mut OUT_BUFFER: &mut [u8] = &mut [0; OUT_BUFFER_SIZE];

            #[no_mangle]
            pub fn #get_in_name() -> *const u8 {
                unsafe { IN_BUFFER.as_ptr() }
            }

            #[no_mangle]
            pub fn #get_out_name() -> *const u8 {
                unsafe { OUT_BUFFER.as_ptr() }
            }

            #[no_mangle]
            pub fn #wrapper_name(in_buffer_len: u32) -> u32 {
                let args = unsafe {
                    DeBin::deserialize_bin(&IN_BUFFER[..in_buffer_len as usize]).unwrap()
                };
                let ret = wrapped(args).serialize_bin();
                unsafe {
                    OUT_BUFFER[..ret.len()].copy_from_slice(&ret);
                }
                ret.len() as u32
            }

            fn wrapped((#pats): (#tys)) #ret {
                #body
            }
        }
    })
}
