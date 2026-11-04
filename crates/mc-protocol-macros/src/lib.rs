//! `#[derive(Encode, Decode, Packet)]` for the `mc-protocol` wire codec.
//!
//! Field attributes (`#[mc(...)]`):
//! * `var` – encode with `VarEncode` (VarInt for `i32`, VarLong for `i64`, also `Vec<i32>`, `Option<i32>`, …)
//! * `max = N` – decode with `DecodeBounded` (string length in UTF-16 units / collection size)
//! * `with = path` – `path::encode(&T, &mut Vec<u8>)` and `path::decode(&mut &[u8]) -> Result<T, DecodeError>`
//!
//! Enum attributes: `#[mc(repr = "varint" | "u8" | "i8" | "u16" | "i16" | "i32" | "i64")]` (default `varint`),
//! `#[mc(error = "Unknown thing: {}")]`; variant attribute `#[mc(id = N)]` (default: previous id + 1, starting at 0).
//! Variants may carry fields, which follow the id.
//!
//! `#[derive(Packet)]` takes `#[packet(state = configuration, flow = clientbound, name = "keep_alive")]`.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Error, Field, Fields, Ident, LitInt, LitStr, Result, Variant, parse_macro_input,
};

const CRATE: &str = "mc_protocol";

fn krate() -> TokenStream2 {
    let id = Ident::new(CRATE, Span::call_site());
    quote!(::#id)
}

#[derive(Default)]
struct FieldAttrs {
    var: bool,
    max: Option<TokenStream2>,
    with: Option<syn::Path>,
}

fn field_attrs(f: &Field) -> Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in f.attrs.iter().filter(|a| a.path().is_ident("mc")) {
        attr.parse_nested_meta(|m| {
            if m.path.is_ident("var") || m.path.is_ident("varint") || m.path.is_ident("varlong") {
                out.var = true;
            } else if m.path.is_ident("max") {
                let e: syn::Expr = m.value()?.parse()?;
                out.max = Some(quote!(#e));
            } else if m.path.is_ident("with") {
                out.with = Some(m.value()?.parse()?);
            } else {
                return Err(m.error("unknown mc field attribute"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

fn encode_expr(f: &Field, access: TokenStream2) -> Result<TokenStream2> {
    let k = krate();
    let a = field_attrs(f)?;
    Ok(if let Some(w) = a.with {
        quote!(#w::encode(#access, out);)
    } else if a.var {
        quote!(#k::codec::VarEncode::encode_var(#access, out);)
    } else {
        quote!(#k::codec::Encode::encode(#access, out);)
    })
}

fn decode_expr(f: &Field) -> Result<TokenStream2> {
    let k = krate();
    let a = field_attrs(f)?;
    let ty = &f.ty;
    Ok(if let Some(w) = a.with {
        quote!(#w::decode(input)?)
    } else if a.var {
        quote!(<#ty as #k::codec::VarEncode>::decode_var(input)?)
    } else if let Some(max) = a.max {
        quote!(<#ty as #k::codec::DecodeBounded>::decode_bounded(input, #max)?)
    } else {
        quote!(<#ty as #k::codec::Decode>::decode(input)?)
    })
}

/// Pattern + encode statements for a set of fields. Bindings are `__f0, __f1, ...`.
fn fields_encode(fields: &Fields) -> Result<(TokenStream2, TokenStream2)> {
    let mut stmts = TokenStream2::new();
    let mut names = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        let b = format_ident!("__f{}", i);
        stmts.extend(encode_expr(f, quote!(#b))?);
        names.push(b);
    }
    let pat = match fields {
        Fields::Unit => quote!(),
        Fields::Unnamed(_) => quote!((#(#names),*)),
        Fields::Named(n) => {
            let ids = n.named.iter().map(|f| f.ident.as_ref().unwrap());
            quote!({ #(#ids: #names),* })
        }
    };
    Ok((pat, stmts))
}

fn fields_decode(fields: &Fields) -> Result<TokenStream2> {
    Ok(match fields {
        Fields::Unit => quote!(),
        Fields::Unnamed(u) => {
            let ex = u.unnamed.iter().map(decode_expr).collect::<Result<Vec<_>>>()?;
            quote!((#(#ex),*))
        }
        Fields::Named(n) => {
            let mut parts = Vec::new();
            for f in &n.named {
                let id = f.ident.as_ref().unwrap();
                let ex = decode_expr(f)?;
                parts.push(quote!(#id: #ex));
            }
            quote!({ #(#parts),* })
        }
    })
}

struct EnumAttrs {
    repr: String,
    error: Option<LitStr>,
}

fn enum_attrs(input: &DeriveInput) -> Result<EnumAttrs> {
    let mut out = EnumAttrs { repr: "varint".into(), error: None };
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("mc")) {
        attr.parse_nested_meta(|m| {
            if m.path.is_ident("repr") {
                out.repr = m.value()?.parse::<LitStr>()?.value();
            } else if m.path.is_ident("error") {
                out.error = Some(m.value()?.parse()?);
            } else {
                return Err(m.error("unknown mc enum attribute"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

fn variant_id(v: &Variant) -> Result<Option<i64>> {
    let mut id = None;
    for attr in v.attrs.iter().filter(|a| a.path().is_ident("mc")) {
        attr.parse_nested_meta(|m| {
            if m.path.is_ident("id") {
                id = Some(m.value()?.parse::<LitInt>()?.base10_parse::<i64>()?);
                Ok(())
            } else {
                Err(m.error("unknown mc variant attribute"))
            }
        })?;
    }
    Ok(id)
}

fn variant_ids(data: &syn::DataEnum) -> Result<Vec<i64>> {
    let mut next = 0i64;
    let mut ids = Vec::new();
    for v in &data.variants {
        let id = variant_id(v)?.unwrap_or(next);
        ids.push(id);
        next = id + 1;
    }
    Ok(ids)
}

#[proc_macro_derive(Encode, attributes(mc))]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_encode(&input).unwrap_or_else(Error::into_compile_error).into()
}

#[proc_macro_derive(Decode, attributes(mc))]
pub fn derive_decode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_decode(&input).unwrap_or_else(Error::into_compile_error).into()
}

fn with_bounds(input: &DeriveInput, bound: TokenStream2) -> (TokenStream2, TokenStream2, TokenStream2) {
    let mut generics = input.generics.clone();
    for tp in generics.type_params_mut() {
        tp.bounds.push(syn::parse2(bound.clone()).unwrap());
    }
    let (impl_g, ty_g, where_c) = generics.split_for_impl();
    (quote!(#impl_g), quote!(#ty_g), quote!(#where_c))
}

fn write_repr(repr: &str, k: &TokenStream2, id: i64) -> Result<TokenStream2> {
    let lit = |t: TokenStream2| quote!(#k::codec::Encode::encode(&(#id as #t), out););
    Ok(match repr {
        "varint" => quote!(#k::codec::write_varint(out, #id as i32);),
        "u8" => lit(quote!(u8)),
        "i8" => lit(quote!(i8)),
        "u16" => lit(quote!(u16)),
        "i16" => lit(quote!(i16)),
        "i32" => lit(quote!(i32)),
        "i64" => lit(quote!(i64)),
        other => return Err(Error::new(Span::call_site(), format!("unknown repr {other}"))),
    })
}

fn read_repr(repr: &str, k: &TokenStream2) -> Result<TokenStream2> {
    let rd = |t: TokenStream2| quote!(<#t as #k::codec::Decode>::decode(input)? as i64);
    Ok(match repr {
        "varint" => quote!(#k::codec::read_varint(input)? as i64),
        "u8" => rd(quote!(u8)),
        "i8" => rd(quote!(i8)),
        "u16" => rd(quote!(u16)),
        "i16" => rd(quote!(i16)),
        "i32" => rd(quote!(i32)),
        "i64" => rd(quote!(i64)),
        other => return Err(Error::new(Span::call_site(), format!("unknown repr {other}"))),
    })
}

fn expand_encode(input: &DeriveInput) -> Result<TokenStream2> {
    let k = krate();
    let name = &input.ident;
    let (impl_g, ty_g, where_c) = with_bounds(input, quote!(#k::codec::Encode));
    let body = match &input.data {
        Data::Struct(s) => {
            let (pat, stmts) = fields_encode(&s.fields)?;
            quote!(let Self #pat = self; #stmts)
        }
        Data::Enum(e) => {
            let attrs = enum_attrs(input)?;
            let ids = variant_ids(e)?;
            let mut arms = Vec::new();
            for (v, id) in e.variants.iter().zip(ids) {
                let vn = &v.ident;
                let (pat, stmts) = fields_encode(&v.fields)?;
                let w = write_repr(&attrs.repr, &k, id)?;
                arms.push(quote!(Self::#vn #pat => { #w #stmts }));
            }
            if arms.is_empty() {
                quote!(match *self {})
            } else {
                quote!(match self { #(#arms)* })
            }
        }
        Data::Union(_) => return Err(Error::new_spanned(input, "unions are not supported")),
    };
    Ok(quote! {
        impl #impl_g #k::codec::Encode for #name #ty_g #where_c {
            #[allow(unused_variables)]
            fn encode(&self, out: &mut ::std::vec::Vec<u8>) { #body }
        }
    })
}

fn expand_decode(input: &DeriveInput) -> Result<TokenStream2> {
    let k = krate();
    let name = &input.ident;
    let (impl_g, ty_g, where_c) = with_bounds(input, quote!(#k::codec::Decode));
    let body = match &input.data {
        Data::Struct(s) => {
            let d = fields_decode(&s.fields)?;
            quote!(::std::result::Result::Ok(Self #d))
        }
        Data::Enum(e) => {
            let attrs = enum_attrs(input)?;
            let ids = variant_ids(e)?;
            let rd = read_repr(&attrs.repr, &k)?;
            let mut arms = Vec::new();
            for (v, id) in e.variants.iter().zip(ids) {
                let vn = &v.ident;
                let d = fields_decode(&v.fields)?;
                arms.push(quote!(#id => ::std::result::Result::Ok(Self::#vn #d),));
            }
            let err = match &attrs.error {
                Some(fmt) => quote!(#k::codec::DecodeError::Message(::std::format!(#fmt, other))),
                None => {
                    let n = name.to_string();
                    quote!(#k::codec::DecodeError::Message(::std::format!("Unknown {} id: {}", #n, other)))
                }
            };
            quote! {
                let id: i64 = #rd;
                match id { #(#arms)* other => ::std::result::Result::Err(#err), }
            }
        }
        Data::Union(_) => return Err(Error::new_spanned(input, "unions are not supported")),
    };
    Ok(quote! {
        impl #impl_g #k::codec::Decode for #name #ty_g #where_c {
            #[allow(unused_variables)]
            fn decode(input: &mut &[u8]) -> ::std::result::Result<Self, #k::codec::DecodeError> { #body }
        }
    })
}

#[proc_macro_derive(Packet, attributes(packet))]
pub fn derive_packet(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_packet(&input).unwrap_or_else(Error::into_compile_error).into()
}

fn expand_packet(input: &DeriveInput) -> Result<TokenStream2> {
    let k = krate();
    let (mut state, mut flow, mut pname) = (None::<Ident>, None::<Ident>, None::<LitStr>);
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("packet")) {
        attr.parse_nested_meta(|m| {
            if m.path.is_ident("state") {
                state = Some(m.value()?.parse()?);
            } else if m.path.is_ident("flow") {
                flow = Some(m.value()?.parse()?);
            } else if m.path.is_ident("name") {
                pname = Some(m.value()?.parse()?);
            } else {
                return Err(m.error("unknown packet attribute"));
            }
            Ok(())
        })?;
    }
    let missing = || Error::new_spanned(input, "#[packet(state = .., flow = .., name = \"..\")] required");
    let (state, flow, pname) = (state.ok_or_else(missing)?, flow.ok_or_else(missing)?, pname.ok_or_else(missing)?);
    let konst = Ident::new(&pname.value().replace('/', "_").to_uppercase(), pname.span());
    let name = &input.ident;
    let state_v = Ident::new(&capitalize(&state.to_string()), state.span());
    let flow_v = Ident::new(&capitalize(&flow.to_string()), flow.span());
    let full = format!("minecraft:{}", pname.value());
    Ok(quote! {
        impl #k::packet::Packet for #name {
            const ID: i32 = #k::ids::#state::#flow::#konst;
            const NAME: &'static str = #full;
            const STATE: #k::packet::ConnectionState = #k::packet::ConnectionState::#state_v;
            const FLOW: #k::packet::PacketFlow = #k::packet::PacketFlow::#flow_v;
        }
    })
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}
