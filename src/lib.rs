#![doc = include_str!("../README.md")]
use proc_macro::TokenStream;
use proc_macro2::{
    Delimiter as Delimiter2, Group as Group2, Span as Span2, TokenStream as TokenStream2,
};
use quote::ToTokens;
use syn::{
    braced, bracketed,
    ext::IdentExt,
    parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::{Pair, Punctuated},
    spanned::Spanned,
    token::{
        Add, Bang, Brace, Comma, Const, Default as DefaultKW, For, Gt, Impl, Lt, Paren, Pound,
        Unsafe,
    },
    AttrStyle, Attribute, BoundLifetimes, ConstParam, Error, Ident, ItemImpl, Lifetime,
    LifetimeDef, ParenthesizedGenericArguments, Path, PathArguments, PredicateEq,
    PredicateLifetime, Result, Token, Type, TypePath,
};
// syn::Generics is not suitable for support of const_trait_impl and const_fn_trait_bound
// due to the two transitive chains:
//
// use syn::Generics;
// use syn::GenericParam;
// use syn::TypeParam;
// use syn::TypeParamBound;
// use syn::TraitBound;
// use syn::TraitBoundModifier;
//
// use syn::Generics;
// use syn::WhereClause;
// use syn::WherePredicate;
// use syn::PredicateType;
// use syn::TypeParamBound;
//
use syn::ImplItem;
//
// TODO: track issue: <https://github.com/dtolnay/syn/issues/1130>

mod to_tokens;

struct ItemConstImpl {
    attrs: Vec<Attribute>,
    // https://github.com/rust-lang/rfcs/blob/master/text/1210-impl-specialization.md
    defaultness: Option<DefaultKW>,
    unsafety: Option<Unsafe>,
    impl_token: Impl,
    generics: Generics,
    constness: Option<Const>,
    trait_: Option<(Option<Bang>, Path, For)>,
    self_ty: Box<Type>,
    brace_token: Brace,
    items: Vec<ImplItem>,
}

// enum ImplItem {
//     /// An associated constant within an impl block.
//     Const(ImplItemConst),
//
//     /// A method within an impl block.
//     Method(ImplItemMethod),
//
//     /// An associated type within an impl block.
//     Type(ImplItemType),
//
//     /// A macro invocation within an impl block.
//     Macro(ImplItemMacro),
//
//     /// Tokens within an impl block not interpreted by Syn.
//     Verbatim(TokenStream),
//
//     // // The following is the only supported idiom for exhaustive matching of
//     // // this enum.
//     // //
//     // //     match expr {
//     // //         ImplItem::Const(e) => {...}
//     // //         ImplItem::Method(e) => {...}
//     // //         ...
//     // //         ImplItem::Verbatim(e) => {...}
//     // //
//     // //         #[cfg(test)]
//     // //         ImplItem::__TestExhaustive(_) => unimplemented!(),
//     // //         #[cfg(not(test))]
//     // //         _ => { /* some sane fallback */ }
//     // //     }
//     // //
//     // // This way we fail your tests but don't break your library when adding
//     // // a variant. You will be notified by a test failure when a variant is
//     // // added, so that you can add code to handle it, but your library will
//     // // continue to compile and work for downstream users in the interim.
//     // //
//     // // Once `deny(reachable)` is available in rustc, ImplItem will be
//     // // reimplemented as a non_exhaustive enum.
//     // // https://github.com/rust-lang/rust/issues/44109#issuecomment-521781237
//     // #[doc(hidden)]
//     // __TestExhaustive(crate::private),
// }

// generics.rs (syn 1.0.86)
#[derive(Default)]
pub(crate) struct Generics {
    lt_token: Option<Lt>,
    params: Punctuated<GenericParam, Comma>,
    gt_token: Option<Gt>,
    where_clause: Option<WhereClause>,
}

// generics.rs (syn 1.0.86)
#[allow(clippy::large_enum_variant)]
pub(crate) enum GenericParam {
    /// A generic type parameter: `T: Into<String>`.
    Type(TypeParam),

    /// A lifetime definition: `'a: 'b + 'c + 'd`.
    Lifetime(LifetimeDef),

    /// A const generic parameter: `const LENGTH: usize`.
    Const(ConstParam),
}

// generics.rs (syn 1.0.86)
struct TypeParam {
    pub attrs: Vec<Attribute>,
    pub ident: Ident,
    pub colon_token: Option<Token![:]>,
    pub bounds: Punctuated<TypeParamBound, Token![+]>,
    pub eq_token: Option<Token![=]>,
    pub default: Option<Type>,
}

// generics.rs (syn 1.0.86)
pub(crate) enum TypeParamBound {
    Trait(TraitBound),
    Lifetime(Lifetime),
}

// generics.rs (syn 1.0.86)
pub(crate) struct TraitBound {
    pub paren_token: Option<Paren>,
    pub modifier: TraitBoundModifier,
    /// The `for<'a>` in `for<'a> Foo<&'a T>`
    pub lifetimes: Option<BoundLifetimes>,
    /// The `Foo<&'a T>` in `for<'a> Foo<&'a T>`
    pub path: Path,
}

// generics.rs (syn 1.0.86)
pub(crate) enum TraitBoundModifier {
    None,
    Maybe(Token![?]),
    TildeConst(TildeConst),
}

pub(crate) struct TildeConst {
    tilde: Token![~],
    const_: Token![const],
}

pub(crate) struct PredicateType {
    /// Any lifetimes from a `for` binding
    pub lifetimes: Option<BoundLifetimes>,
    /// The type being bounded
    pub bounded_ty: Type,
    pub colon_token: Token![:],
    /// Trait and lifetime bounds (`Clone+Send+'static`)
    pub bounds: Punctuated<TypeParamBound, Token![+]>,
}

// generics.rs (syn 1.0.86)
pub(crate) enum WherePredicate {
    /// A type predicate in a `where` clause: `for<'c> Foo<'c>: Trait<'c>`.
    Type(PredicateType),

    /// A lifetime predicate in a `where` clause: `'a: 'b + 'c`.
    Lifetime(PredicateLifetime),

    /// An equality predicate in a `where` clause (unsupported).
    #[allow(dead_code)]
    Eq(PredicateEq),
}

// generics.rs (syn 1.0.86)
pub(crate) struct WhereClause {
    pub where_token: Token![where],
    pub predicates: Punctuated<WherePredicate, Token![,]>,
}

impl From<PredicateType> for syn::PredicateType {
    fn from(
        PredicateType {
            lifetimes,
            bounded_ty,
            colon_token,
            bounds,
        }: PredicateType,
    ) -> Self {
        Self {
            lifetimes,
            bounded_ty,
            colon_token,
            bounds: bounds
                .into_pairs()
                .filter_map(|pair| {
                    let drop_bound_filter_map = |b: TypeParamBound| {
                        if let TypeParamBound::Trait(tb) = b {
                            let TraitBound {
                                paren_token,
                                modifier,
                                lifetimes,
                                path,
                            } = tb;
                            match modifier {
                                TraitBoundModifier::TildeConst(tc) => {
                                    if path.segments.last().unwrap().ident.to_string() == "Drop" {
                                        None
                                    } else {
                                        let modifier = TraitBoundModifier::TildeConst(tc);
                                        let tb = TraitBound {
                                            paren_token,
                                            modifier,
                                            lifetimes,
                                            path,
                                        };
                                        Some(TypeParamBound::Trait(tb))
                                    }
                                }
                                _ => {
                                    let tb = TraitBound {
                                        paren_token,
                                        modifier,
                                        lifetimes,
                                        path,
                                    };
                                    Some(TypeParamBound::Trait(tb))
                                }
                            }
                        } else {
                            Some(b)
                        }
                    };
                    match pair {
                        Pair::<TypeParamBound, Add>::Punctuated(b, add) => drop_bound_filter_map(b)
                            .map(|b| Pair::<TypeParamBound, Add>::Punctuated(b, add)),
                        Pair::<TypeParamBound, Add>::End(b) => {
                            drop_bound_filter_map(b).map(Pair::<TypeParamBound, Add>::End)
                        }
                    }
                })
                .map(|pair| match pair {
                    Pair::<TypeParamBound, Add>::Punctuated(b, add) => {
                        Pair::<syn::TypeParamBound, Add>::Punctuated(b.into(), add)
                    }
                    Pair::<TypeParamBound, Add>::End(b) => {
                        Pair::<syn::TypeParamBound, Add>::End(b.into())
                    }
                })
                .collect::<Punctuated<syn::TypeParamBound, Add>>(),
        }
    }
}

// generics.rs (syn 1.0.86)
// Originally, the code was generated with a macro
impl Parse for WherePredicate {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Lifetime) && input.peek2(syn::token::Colon) {
            Ok(WherePredicate::Lifetime(PredicateLifetime {
                lifetime: input.parse()?,
                colon_token: input.parse()?,
                bounds: {
                    let mut bounds = Punctuated::new();
                    loop {
                        if input.is_empty()
                            || input.peek(syn::token::Brace)
                            || input.peek(syn::token::Comma)
                            || input.peek(syn::token::Semi)
                            || input.peek(syn::token::Colon)
                            || input.peek(syn::token::Eq)
                        {
                            break;
                        }
                        let value = input.parse()?;
                        bounds.push_value(value);
                        if !input.peek(syn::token::Add) {
                            break;
                        }
                        let punct = input.parse()?;
                        bounds.push_punct(punct);
                    }
                    bounds
                },
            }))
        } else {
            Ok(WherePredicate::Type(PredicateType {
                lifetimes: input.parse()?,
                bounded_ty: input.parse()?,
                colon_token: input.parse()?,
                bounds: {
                    let mut bounds = Punctuated::new();
                    loop {
                        if input.is_empty()
                            || input.peek(syn::token::Brace)
                            || input.peek(syn::token::Comma)
                            || input.peek(syn::token::Semi)
                            || input.peek(syn::token::Colon) && !input.peek(syn::token::Colon2)
                            || input.peek(syn::token::Eq)
                        {
                            break;
                        }
                        let value = input.parse()?;
                        bounds.push_value(value);
                        if !input.peek(syn::token::Add) {
                            break;
                        }
                        let punct = input.parse()?;
                        bounds.push_punct(punct);
                    }
                    bounds
                },
            }))
        }
    }
}

// generics.rs (syn 1.0.86)
// Originally, the code was generated with a macro
impl Parse for WhereClause {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(WhereClause {
            where_token: input.parse()?,
            predicates: {
                let mut predicates = Punctuated::new();
                loop {
                    if input.is_empty()
                        || input.peek(syn::token::Brace)
                        || input.peek(syn::token::Comma)
                        || input.peek(syn::token::Semi)
                        || input.peek(syn::token::Colon) && !input.peek(syn::token::Colon2)
                        || input.peek(syn::token::Eq)
                    {
                        break;
                    }
                    let value = input.parse::<WherePredicate>()?;
                    predicates.push_value(value);
                    if !input.peek(syn::token::Comma) {
                        break;
                    }
                    let punct = input.parse()?;
                    predicates.push_punct(punct);
                }
                predicates
            },
        })
    }
}

trait LocalParse: Sized {
    fn local_parse(input: ParseStream) -> Result<Self>;
}

impl LocalParse for Option<WhereClause> {
    fn local_parse(input: ParseStream) -> Result<Self> {
        if input.peek(syn::token::Where) {
            input.parse().map(Some)
        } else {
            Ok(None)
        }
    }
}

impl Parse for TildeConst {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            tilde: input.parse::<Token![~]>()?,
            const_: input.parse::<Token![const]>()?,
        })
    }
}

// generics.rs (syn 1.0.86)
impl Parse for TraitBoundModifier {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![?]) {
            input.parse().map(TraitBoundModifier::Maybe)
        } else if input.peek(Token![~]) && input.peek2(Token![const]) {
            input.parse().map(TraitBoundModifier::TildeConst)
        } else {
            Ok(TraitBoundModifier::None)
        }
    }
}

// generics.rs (syn 1.0.86)
impl Parse for TraitBound {
    fn parse(input: ParseStream) -> Result<Self> {
        let modifier: TraitBoundModifier = input.parse()?;
        let lifetimes: Option<BoundLifetimes> = input.parse()?;

        let mut path: Path = input.parse()?;
        if path.segments.last().unwrap().arguments.is_empty()
            && (input.peek(Paren) || input.peek(Token![::]) && input.peek3(Paren))
        {
            input.parse::<Option<Token![::]>>()?;
            let args: ParenthesizedGenericArguments = input.parse()?;
            let parenthesized = PathArguments::Parenthesized(args);
            path.segments.last_mut().unwrap().arguments = parenthesized;
        }

        // {
        //     if let TraitBoundModifier::TildeConst(TildeConst {
        //         tilde,
        //         const_,
        //     }) = modifier
        //     {
        //         path.segments.insert(
        //             0,
        //             PathSegment {
        //                 ident: Ident::new("const", const_.span),
        //                 arguments: PathArguments::None,
        //             },
        //         );
        //         let (_const, punct) = path.segments.pairs_mut().next().unwrap().into_tuple();
        //         *punct.unwrap() = Token![::](tilde.span);
        //     }
        // }

        Ok(TraitBound {
            paren_token: None,
            modifier,
            lifetimes,
            path,
        })
    }
}

// generics.rs (syn 1.0.86)
impl Parse for TypeParamBound {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Lifetime) {
            return input.parse().map(TypeParamBound::Lifetime);
        }

        if input.peek(Paren) {
            let content;
            let paren_token = parenthesized!(content in input);
            let mut bound: TraitBound = content.parse::<TraitBound>()?;
            bound.paren_token = Some(paren_token);
            return Ok(TypeParamBound::Trait(bound));
        }

        input.parse::<TraitBound>().map(TypeParamBound::Trait)
    }
}

// // verbatim.rs (syn 1.0.86)
// mod verbatim {
//     use super::*;
//     pub fn between<'a>(begin: ParseBuffer<'a>, end: ParseStream<'a>) -> TokenStream2 {
//         let end = end.cursor();
//         let mut cursor = begin.cursor();
//         let mut tokens = TokenStream2::new();
//         while cursor != end {
//             let (tt, next) = cursor.token_tree().unwrap();
//             tokens.extend(core::iter::once(tt));
//             cursor = next;
//         }
//         tokens
//     }
// }

// generics.rs (syn 1.0.86)
impl Parse for TypeParam {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let ident: Ident = input.parse()?;
        let colon_token: Option<Token![:]> = input.parse()?;

        // let begin_bound = input.fork();
        // let mut is_maybe_const = false;
        let mut bounds = Punctuated::new();
        if colon_token.is_some() {
            loop {
                if input.peek(Token![,]) || input.peek(Token![>]) || input.peek(Token![=]) {
                    break;
                }
                let value: TypeParamBound = input.parse::<TypeParamBound>()?;

                match &value {
                    TypeParamBound::Lifetime(_) => {}
                    TypeParamBound::Trait(_trait_) => {
                        //if let TraitBoundModifier::TildeConst(_) = trait_.modifier {
                        //    is_maybe_const = true;
                        //}
                    }
                }

                bounds.push_value(value);
                if !input.peek(Token![+]) {
                    break;
                }
                let punct: Token![+] = input.parse()?;
                bounds.push_punct(punct);
            }
        }

        let eq_token: Option<Token![=]> = input.parse()?;
        let default = if eq_token.is_some() {
            Some(input.parse::<Type>()?)
        } else {
            None
        };

        // if is_maybe_const {
        //     bounds.clear();
        //     eq_token = None;
        //     default = Some(Type::Verbatim(verbatim::between(begin_bound, input)));
        // }

        Ok(TypeParam {
            attrs,
            ident,
            colon_token,
            bounds,
            eq_token,
            default,
        })
    }
}

// generics.rs (syn 1.0.86)
impl Parse for Generics {
    fn parse(input: ParseStream) -> Result<Self> {
        if !input.peek(Token![<]) {
            return Ok(Generics::default());
        }

        let lt_token: Token![<] = input.parse()?;

        let mut params = Punctuated::new();
        loop {
            if input.peek(Token![>]) {
                break;
            }

            let attrs = input.call(Attribute::parse_outer)?;
            let lookahead = input.lookahead1();
            if lookahead.peek(Lifetime) {
                params.push_value(GenericParam::Lifetime(LifetimeDef {
                    attrs,
                    ..input.parse()?
                }));
            } else if lookahead.peek(Ident) {
                params.push_value(GenericParam::Type(TypeParam {
                    attrs,
                    ..input.parse::<TypeParam>()?
                }));
            } else if lookahead.peek(Token![const]) {
                params.push_value(GenericParam::Const(ConstParam {
                    attrs,
                    ..input.parse::<ConstParam>()?
                }));
            } else if input.peek(Token![_]) {
                params.push_value(GenericParam::Type(TypeParam {
                    attrs,
                    ident: input.call(Ident::parse_any)?,
                    colon_token: None,
                    bounds: Punctuated::new(),
                    eq_token: None,
                    default: None,
                }));
            } else {
                return Err(lookahead.error());
            }

            if input.peek(Token![>]) {
                break;
            }
            let punct = input.parse()?;
            params.push_punct(punct);
        }

        let gt_token: Token![>] = input.parse()?;

        Ok(Generics {
            lt_token: Some(lt_token),
            params,
            gt_token: Some(gt_token),
            where_clause: None,
        })
    }
}

pub(crate) struct TokensOrDefault<'a, T: 'a>(pub &'a Option<T>);

// syn::attr::parsing::parse_inner (syn 1.0.86)
#[allow(clippy::eval_order_dependence)]
fn single_parse_inner(input: ParseStream) -> Result<Attribute> {
    let content;
    Ok(Attribute {
        pound_token: input.parse::<Pound>()?,
        style: AttrStyle::Inner(input.parse::<Bang>()?),
        bracket_token: bracketed!(content in input),
        path: content.call(Path::parse_mod_style)?,
        tokens: content.parse::<TokenStream2>()?,
    })
}

// syn::attr::parsing::parse_inner (syn 1.0.86)
fn parse_inner(input: ParseStream, attrs: &mut Vec<Attribute>) -> Result<()> {
    while input.peek(Token![#]) && input.peek2(Token![!]) {
        attrs.push(input.call(single_parse_inner)?);
    }
    Ok(())
}

impl Parse for ItemConstImpl {
    // Largely based on: https://docs.rs/syn/1.0.86/src/syn/item.rs.html#2402-2407
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut attrs = input.call(Attribute::parse_outer)?;
        let defaultness = input.parse::<Option<DefaultKW>>()?;
        let unsafety = input.parse::<Option<Token![unsafe]>>()?;
        let impl_token = input.parse::<Impl>()?;

        let has_generics = input.peek(Token![<])
            && (input.peek2(Token![>])
                || input.peek2(Token![#])
                || (input.peek2(Ident) || input.peek2(Lifetime))
                    && (input.peek3(Token![:])
                        || input.peek3(Token![,])
                        || input.peek3(Token![>])
                        || input.peek3(Token![=]))
                || input.peek2(Token![const]));
        let mut generics: Generics = if has_generics {
            input.parse::<Generics>()?
        } else {
            Generics::default()
        };

        let is_const_impl = input.peek(Token![const]);
        // The author is uncertain where the second kind of const impl comes from
        // || input.peek(Token![?]) && input.peek2(Token![const]);
        let constness = if is_const_impl {
            // input.parse::<Option<Token![?]>>()?;
            Some(input.parse::<Token![const]>()?)
        } else {
            None
        };
        let polarity = if input.peek(Token![!]) && !input.peek2(Brace) {
            Some(input.parse::<Token![!]>()?)
        } else {
            None
        };
        let first_ty_span = input.span();
        let mut first_ty: Type = input.parse::<Type>()?;
        let self_ty: Type;
        let trait_;

        let is_impl_for = input.peek(Token![for]);
        if is_impl_for {
            let for_token: Token![for] = input.parse::<Token![for]>()?;
            let mut first_ty_ref = &first_ty;
            while let Type::Group(ty) = first_ty_ref {
                first_ty_ref = &ty.elem;
            }
            if let Type::Path(TypePath { qself: None, .. }) = first_ty_ref {
                while let Type::Group(ty) = first_ty {
                    first_ty = *ty.elem;
                }
                if let Type::Path(TypePath { qself: None, path }) = first_ty {
                    trait_ = Some((polarity, path, for_token));
                } else {
                    unreachable!();
                }
            } else {
                return Err(Error::new(first_ty_span, "expected trait path"));
            }
            self_ty = input.parse::<Type>()?;
        } else {
            return Err(Error::new(Span2::call_site(), "expected trait impl block"));
        };
        generics.where_clause = Option::<WhereClause>::local_parse(input)?;

        let content;
        let brace_token = braced!(content in input);
        parse_inner(&content, &mut attrs)?;

        let mut items = Vec::new();
        while !content.is_empty() {
            items.push(content.parse::<ImplItem>()?);
        }
        if is_impl_for && trait_.is_none() {
            Err(Error::new(is_impl_for.span(), "expected trait name"))
        } else {
            Ok(ItemConstImpl {
                attrs,
                defaultness,
                unsafety,
                impl_token,
                generics,
                constness,
                trait_,
                self_ty: Box::new(self_ty),
                brace_token,
                items,
            })
        }
    }
}

impl From<TraitBoundModifier> for syn::TraitBoundModifier {
    fn from(m: TraitBoundModifier) -> Self {
        match m {
            TraitBoundModifier::None | TraitBoundModifier::TildeConst(_) => {
                syn::TraitBoundModifier::None
            }
            TraitBoundModifier::Maybe(question) => syn::TraitBoundModifier::Maybe(question),
        }
    }
}

impl From<TraitBound> for syn::TraitBound {
    fn from(b: TraitBound) -> Self {
        let TraitBound {
            paren_token,
            modifier,
            lifetimes,
            path,
        } = b;
        Self {
            paren_token,
            modifier: modifier.into(),
            lifetimes,
            path,
        }
    }
}

impl From<TypeParamBound> for syn::TypeParamBound {
    fn from(b: TypeParamBound) -> Self {
        match b {
            TypeParamBound::Lifetime(l) => Self::Lifetime(l),
            TypeParamBound::Trait(t) => Self::Trait(t.into()),
        }
    }
}

impl From<TypeParam> for syn::TypeParam {
    fn from(t: TypeParam) -> Self {
        let TypeParam {
            attrs,
            ident,
            colon_token,
            bounds,
            eq_token,
            default,
        } = t;
        Self {
            attrs,
            ident,
            colon_token,
            bounds: bounds
                .into_pairs()
                .filter_map(|pair| {
                    let drop_bound_filter_map = |b: TypeParamBound| {
                        if let TypeParamBound::Trait(tb) = b {
                            let TraitBound {
                                paren_token,
                                modifier,
                                lifetimes,
                                path,
                            } = tb;
                            match modifier {
                                TraitBoundModifier::TildeConst(tc) => {
                                    if path.segments.last().unwrap().ident.to_string() == "Drop" {
                                        None
                                    } else {
                                        let modifier = TraitBoundModifier::TildeConst(tc);
                                        let tb = TraitBound {
                                            paren_token,
                                            modifier,
                                            lifetimes,
                                            path,
                                        };
                                        Some(TypeParamBound::Trait(tb))
                                    }
                                }
                                _ => {
                                    let tb = TraitBound {
                                        paren_token,
                                        modifier,
                                        lifetimes,
                                        path,
                                    };
                                    Some(TypeParamBound::Trait(tb))
                                }
                            }
                        } else {
                            Some(b)
                        }
                    };
                    match pair {
                        Pair::<TypeParamBound, Add>::Punctuated(b, add) => drop_bound_filter_map(b)
                            .map(|b| Pair::<TypeParamBound, Add>::Punctuated(b, add)),
                        Pair::<TypeParamBound, Add>::End(b) => {
                            drop_bound_filter_map(b).map(Pair::<TypeParamBound, Add>::End)
                        }
                    }
                })
                .map(|pair| match pair {
                    Pair::<TypeParamBound, Add>::Punctuated(b, add) => {
                        Pair::<syn::TypeParamBound, Add>::Punctuated(b.into(), add)
                    }
                    Pair::<TypeParamBound, Add>::End(b) => {
                        Pair::<syn::TypeParamBound, Add>::End(b.into())
                    }
                })
                .collect::<Punctuated<syn::TypeParamBound, Add>>(),
            eq_token,
            default,
        }
    }
}

impl From<GenericParam> for syn::GenericParam {
    fn from(param: GenericParam) -> Self {
        match param {
            GenericParam::Const(c) => syn::GenericParam::Const(c),
            GenericParam::Lifetime(l) => syn::GenericParam::Lifetime(l),
            GenericParam::Type(t) => syn::GenericParam::Type(t.into()),
        }
    }
}

impl From<WherePredicate> for syn::WherePredicate {
    fn from(predicate: WherePredicate) -> Self {
        match predicate {
            WherePredicate::Eq(eq) => syn::WherePredicate::Eq(eq),
            WherePredicate::Lifetime(lt) => syn::WherePredicate::Lifetime(lt),
            WherePredicate::Type(ty) => syn::WherePredicate::Type(ty.into()),
        }
    }
}

impl From<WhereClause> for syn::WhereClause {
    fn from(
        WhereClause {
            where_token,
            predicates,
        }: WhereClause,
    ) -> Self {
        Self {
            where_token,
            predicates: predicates
                .into_pairs()
                .map(|pair| match pair {
                    Pair::<WherePredicate, Comma>::Punctuated(p, comma) => {
                        Pair::<syn::WherePredicate, Comma>::Punctuated(p.into(), comma)
                    }
                    Pair::<WherePredicate, Comma>::End(p) => {
                        Pair::<syn::WherePredicate, Comma>::End(p.into())
                    }
                })
                .collect::<Punctuated<syn::WherePredicate, Comma>>(),
        }
    }
}

impl From<Generics> for syn::Generics {
    fn from(generics: Generics) -> Self {
        let Generics {
            lt_token,
            params,
            gt_token,
            where_clause,
        } = generics;
        // The code below reallocates. Fix it later
        Self {
            lt_token,
            params: params
                .into_pairs()
                .map(|pair| match pair {
                    Pair::<GenericParam, Comma>::Punctuated(p, comma) => {
                        Pair::<syn::GenericParam, Comma>::Punctuated(p.into(), comma)
                    }
                    Pair::<GenericParam, Comma>::End(p) => {
                        Pair::<syn::GenericParam, Comma>::End(p.into())
                    }
                })
                .collect::<Punctuated<syn::GenericParam, Comma>>(),
            gt_token,
            where_clause: where_clause.map(<WhereClause as Into<syn::WhereClause>>::into),
        }
    }
}

impl From<ItemConstImpl> for ItemImpl {
    fn from(item_const_impl: ItemConstImpl) -> Self {
        let ItemConstImpl {
            attrs,
            defaultness,
            unsafety,
            impl_token,
            generics,
            constness: _constness,
            trait_,
            self_ty,
            brace_token,
            items,
        } = item_const_impl;
        Self {
            attrs,
            defaultness,
            unsafety,
            impl_token,
            generics: generics.into(),
            trait_,
            self_ty,
            brace_token,
            items,
        }
    }
}

impl From<ItemConstImpl> for TokenStream {
    #[allow(unused_variables, clippy::let_and_return)]
    fn from(item_impl: ItemConstImpl) -> TokenStream {
        let ItemConstImpl {
            attrs,
            defaultness,
            unsafety,
            impl_token,
            generics,
            constness,
            trait_,
            self_ty,
            brace_token,
            items,
        } = item_impl;
        let Generics {
            lt_token,
            gt_token,
            params,
            where_clause,
        } = generics;
        let mut ts = TokenStream::new();
        for attr in attrs.into_iter() {
            ts.extend::<TokenStream>(attr.to_token_stream().into());
        }
        ts.extend::<TokenStream>(defaultness.to_token_stream().into());
        ts.extend::<TokenStream>(unsafety.to_token_stream().into());
        ts.extend::<TokenStream>(impl_token.to_token_stream().into());
        ts.extend::<TokenStream>(lt_token.to_token_stream().into());
        ts.extend::<TokenStream>(params.to_token_stream().into());
        ts.extend::<TokenStream>(gt_token.to_token_stream().into());
        ts.extend::<TokenStream>(constness.to_token_stream().into());
        match trait_ {
            None => {}
            Some((bang, path, for_)) => {
                ts.extend::<TokenStream>(bang.to_token_stream().into());
                ts.extend::<TokenStream>(path.to_token_stream().into());
                ts.extend::<TokenStream>(for_.to_token_stream().into());
            }
        };
        ts.extend::<TokenStream>(self_ty.to_token_stream().into());
        ts.extend::<TokenStream>(where_clause.to_token_stream().into());
        let mut nested_ts = TokenStream2::new();
        for item in items.into_iter() {
            nested_ts.extend(item.to_token_stream());
        }
        ts.extend::<TokenStream>(
            Group2::new(Delimiter2::Brace, nested_ts)
                .to_token_stream()
                .into(),
        );
        ts
    }
}

// trait ToDbgString {
//     fn to_dbg_string(&self) -> String;
// }
//
// impl<T> ToDbgString for T
// where
//     T: ToTokens
// {
//     fn to_dbg_string(&self) -> String {
//         let ts = self.to_token_stream();
//         let t_name = core::any::type_name::<T>();
//         format!("{t_name}{{{ts}}}")
//     }
// }

/// Unconditionally turns const trait implementation into non-const
///
/// # Example
///
/// ```rust, ignore
/// #![cfg_attr(feature = "const_trait_impl", feature(const_trait_impl))]
/// #![cfg_attr(feature = "const_default_impls", feature(const_default_impls))]
/// #![cfg_attr(feature = "const_fn_trait_bound", feature(const_fn_trait_bound))]
///
/// #[cfg(not(all(
///     feature = "const_trait_impl",
///     feature = "const_default_impls",
///     feature = "const_fn_trait_bound"
/// )))]
/// use unconst_trait_impl::unconst_trait_impl;
/// use core::{default::Default, marker::PhantomData};
/// #[cfg(all(
///     feature = "const_trait_impl",
///     feature = "const_default_impls",
///     feature = "const_fn_trait_bound"
/// ))]
/// use remove_macro_call::remove_macro_call;
///
/// // Since ZST is both Eq and and PartialEq, it has structural match
/// // https://github.com/rust-lang/rust/issues/63438
/// #[derive(Clone, Debug, Hash, Eq, Ord, PartialEq, PartialOrd, Copy)]
/// pub struct ZST<T: ?Sized>(PhantomData<T>);
///
/// pub trait TraitName {}
///
/// #[cfg_attr(
///     all(
///         feature = "const_trait_impl",
///         feature = "const_default_impls",
///         feature = "const_fn_trait_bound"
///     ),
///     remove_macro_call
/// )]
/// unconst_trait_impl! {
///     impl<T: ?Sized> const TraitName for ZST<T> {}
/// }
///
/// // With `cargo build --features const_trait_impl, const_default_impls, const_fn_trait_bound`
/// // or with `cargo build --all-features, the code below is expanded as is. Otherwise,
/// // it gets "unconsted" to be supported by stable toolchain.
/// #[cfg_attr(
///     all(
///         feature = "const_trait_impl",
///         feature = "const_default_impls",
///         feature = "const_fn_trait_bound"
///     ),
///     remove_macro_call
/// )]
/// unconst_trait_impl! {
///     impl<T: ~const TraitName + ?Sized> const Default for ZST<T> {
///         fn default() -> Self {
///             ZST(Default::default())
///         }
///     }
/// }
/// ```
///
/// **Note**: In the real code, the example above could be replaced with a simpler version relying on [`cfg_aliases`](https://crates.io/crates/cfg_aliases) crate.
///
/// You can learn more about `remove_macro_call` here:
/// * [GitHub](https://github.com/JohnScience/remove_macro_call)
/// * [crates.io](https://crates.io/crates/remove_macro_call)

#[proc_macro]
pub fn unconst_trait_impl(item: TokenStream) -> TokenStream {
    let item_const_impl = parse_macro_input!(item as ItemConstImpl);
    let item_impl: ItemImpl = item_const_impl.into();

    item_impl.to_token_stream().into()

    // let ItemImpl {
    //     attrs,
    //     defaultness,
    //     unsafety,
    //     impl_token,
    //     generics,
    //     trait_,
    //     self_ty,
    //     brace_token,
    //     items
    // } = item_impl;
    // let comment = format!("const S: &str = \"{}\";", generics.to_dbg_string());
    // let ts = <TokenStream as std::str::FromStr>::from_str(&comment).unwrap();
    // ts
}
