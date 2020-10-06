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
    let body = item.block;
    let fn_name = item.sig.ident;
    let mod_name = syn::Ident::new(&format!("_distilled_mod_{}", fn_name), fn_name.span());
    let wrapper_name = syn::Ident::new(&format!("_distilled_wrapper_{}", fn_name), fn_name.span());
    let get_in_name = syn::Ident::new(&format!("_distilled_get_in_{}", fn_name), fn_name.span());
    let get_out_name = syn::Ident::new(&format!("_distilled_get_out_{}", fn_name), fn_name.span());

    TokenStream::from(quote! {
        #[cfg(not(target_arch = "wasm32"))]
        pub fn wrapper () {

        }

        #[cfg(target_arch = "wasm32")]
        mod #mod_name {
            use ::nanoserde::{DeJson, DeJsonState, SerJson};
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
                let passed_string = unsafe { ::std::str::from_utf8(&IN_BUFFER[..in_buffer_len as usize]).unwrap() };
                let args = {
                    let mut state = DeJsonState::default();
                    let mut chars = passed_string.chars();
                    state.next(&mut chars);
                    state.next_tok(&mut chars).expect("deser2");
                    DeJson::de_json(&mut state, &mut chars).expect("deser")
                };

                let ret = wrapped(args).serialize_json();

                unsafe {
                    OUT_BUFFER[..ret.len()].copy_from_slice(ret.as_bytes());
                }
                ret.len() as u32
            }

            fn wrapped((#pats): (#tys)) #ret {
                #body
            }
        }
    })
}
