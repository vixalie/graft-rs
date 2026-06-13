//! # graft-derive
//!
//! Derived macros for `graft`.
//!
//! ## `#[derive(InsertRow)]`
//!
//! Generates `FromRow::insert_values()` for struct fields.
//!
//! ```rust,ignore
//! use graft::InsertRow;
//!
//! #[derive(InsertRow)]
//! struct User {
//!     name: String,
//!     age: i32,
//!     dept: String,
//! }
//!
//! // Generates:
//! // impl FromRow for User {
//! //     fn insert_values(&self) -> Vec<Param> {
//! //         vec![
//! //             self.name.clone().into(),
//! //             self.age.into(),
//! //             self.dept.clone().into(),
//! //         ]
//! //     }
//! // }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

/// 为结构体生成 `FromRow` trait 实现。
///
/// 要求：
/// - 只能用于 `struct`（不支持 enum）
/// - 所有字段必须实现 `Into<Param>`
/// - 跳过带 `#[insert_row(skip)]` 属性的字段
#[proc_macro_derive(InsertRow, attributes(insert_row))]
pub fn derive_insert_row(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let fields = match input.data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => &fields.named,
            _ => {
                return syn::Error::new(
                    name.span(),
                    "InsertRow only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new(
                name.span(),
                "InsertRow only supports structs",
            )
            .to_compile_error()
            .into();
        }
    };

    let conversions: Vec<_> = fields
        .iter()
        .filter(|f| {
            // Skip fields with #[insert_row(skip)]
            !f.attrs.iter().any(|attr| {
                if let Some(ident) = attr.path().get_ident() {
                    ident == "insert_row"
                } else {
                    false
                }
            })
        })
        .map(|f| {
            let field_name = &f.ident;
            quote! {
                self.#field_name.clone().into()
            }
        })
        .collect();

    let expanded = quote! {
        impl graft::FromRow for #name {
            fn insert_values(&self) -> Vec<graft::Param> {
                vec![
                    #(#conversions),*
                ]
            }
        }
    };

    expanded.into()
}
