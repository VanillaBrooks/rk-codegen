use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream, Result};
use syn::spanned::Spanned;
use syn::Lit;
use syn::{parse_macro_input, Expr, ExprArray, Ident, Token};

#[derive(Debug)]
struct RkMethod {
    name: Ident,
    time_shifts: ExprArray,
    space_shifts: ExprArray,
    k_shifts: ExprArray,
}

impl Parse for RkMethod {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let time_shifts: ExprArray = input.parse()?;
        input.parse::<Token![,]>()?;
        let space_shifts: ExprArray = input.parse()?;
        input.parse::<Token![,]>()?;
        let k_shifts: ExprArray = input.parse()?;

        let out = Self {
            name,
            time_shifts,
            space_shifts,
            k_shifts,
        };
        Ok(out)
    }
}

#[derive(Debug)]
struct RkValues {
    name: Ident,
    time_shifts: Vec<Expr>,
    space_shifts: Vec<Vec<Expr>>,
    k_shifts: Vec<Expr>,
}

impl RkValues {
    fn from(raw: RkMethod) -> Self {
        let name = raw.name;
        let time_shifts: Vec<Expr> = raw
            .time_shifts
            .elems
            .into_iter()
            .map(|mut x| {
                make_float(&mut x);
                x
            })
            .collect();

        let k_shifts: Vec<Expr> = raw
            .k_shifts
            .elems
            .into_iter()
            .map(|mut x| {
                make_float(&mut x);
                x
            })
            .collect();

        let space_shifts: Vec<Vec<Expr>> = raw
            .space_shifts
            .elems
            .into_iter()
            .map(|row| {
                if let Expr::Array(row) = row {
                    row.elems
                        .into_iter()
                        .map(|mut x| {
                            make_float(&mut x);
                            x
                        })
                        .collect()
                } else {
                    unreachable!()
                }
            })
            .collect();
        RkValues {
            name,
            time_shifts,
            space_shifts,
            k_shifts,
        }
    }
}

#[proc_macro]
pub fn rk_tableaux(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as RkMethod);

    // we have parsed the input as an RK method, now we need to validate that it is an explicit
    // method

    let mut expected = 0;
    for row in &input.space_shifts.elems {
        if let Expr::Array(row_space_shifts) = row {
            expected += 1;
            assert_eq!(row_space_shifts.elems.len(), expected);
        } else {
            //quote_spanned!(row.span() => panic!("error"));
            panic!("was not an array");
        }
    }

    println!("finsihed space validation");

    assert_eq!(expected, input.time_shifts.elems.len());
    assert_eq!(expected, input.k_shifts.elems.len());

    let values = RkValues::from(input);

    // Calculate k values
    let mut k_idx = 0;
    let mut k_calculations = quote!();
    for (space_shift_row, time_shift) in values.space_shifts.iter().zip(values.time_shifts.iter()) {
        k_idx += 1;

        let name = make_k(k_idx, time_shift.span());

        let expansion = make_space_shift_expansion(space_shift_row, time_shift.span());

        let s = quote!(
            let #name : V = f(tn + (h * #time_shift), yn + #expansion);
        );

        k_calculations = quote!(
            #k_calculations

            #s
        );
    }

    // do the final calculation for yn+1 and create a function surrounding it
    let ynp1 = calculate_ystep(&values.k_shifts);
    let name = values.name;
    let calculation = quote!(
        fn #name<V ,T> (h: f64, yn: V, tn:f64, f: T) -> V
        where V: std::ops::Add<f64, Output=V>
                + std::ops::Add<V, Output=V>
                + std::ops::Mul<f64, Output=V>
                + std::ops::Div<f64, Output=V> +  Copy,
              T:Fn(f64, V) -> V
        {
            #k_calculations

            let ynp1: V = yn + (#ynp1) * h;
            return ynp1
        }
    );

    calculation.into()
}

fn make_k(idx: u8, span: Span) -> Ident {
    Ident::new(&format!("k{}", idx), span)
}

fn make_space_shift_expansion(space_shift_row: &Vec<Expr>, span: Span) -> proc_macro2::TokenStream {
    let mut expansion = None;
    let mut expansion_idx = 0;

    for multiplier in space_shift_row.iter() {
        expansion_idx += 1;
        if is_zero(multiplier) {
            continue;
        }

        let k = make_k(expansion_idx, span);
        if let Some(inner_expansion) = expansion {
            expansion = Some(quote!(
                #inner_expansion + (#k * #multiplier)
            ))
        } else {
            expansion = Some(quote!(
            (#k * #multiplier)
            ));
        }
    }

    if let Some(inner_expansion) = expansion {
        quote!((#inner_expansion)*h)
    } else {
        quote!(0.)
    }
}

fn calculate_ystep(k_shifts: &Vec<Expr>) -> proc_macro2::TokenStream {
    let mut expansion_idx = 0;
    let mut expansion = None;

    for shift in k_shifts {
        expansion_idx += 1;
        if is_zero(shift) {
            continue;
        }

        let k = make_k(expansion_idx, shift.span());

        let new = quote!(#k * #shift);

        if let Some(inner_expansion) = expansion {
            expansion = Some(quote!(
                    #inner_expansion + (#new)));
        } else {
            expansion = Some(quote!((#new)))
        }
    }

    expansion.unwrap_or(quote!(0.))
}

fn is_zero(input: &Expr) -> bool {
    if let Expr::Lit(lit) = input {
        match &lit.lit {
            Lit::Int(litint) => {
                if litint.base10_digits() == "0" {
                    true
                } else {
                    false
                }
            }
            Lit::Float(lit_float) => {
                if lit_float.base10_digits() == "0." {
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    } else {
        false
    }
}

// convert any integer input expressions into floating point input expressions
fn make_float(input_expr: &mut Expr) {
    match input_expr {
        Expr::Lit(expr_lit) => {
            // if the literal is an integer then map it to a float by adding a decimail point
            match &expr_lit.lit {
                Lit::Int(lit_int) => {
                    let digits = lit_int.base10_digits();

                    expr_lit.lit =
                        Lit::Float(syn::LitFloat::new(&format!("{}.", digits), lit_int.span()));
                }
                _ => (),
            }
        }
        // if there was a fraction then map both the LHS and RHS to floats
        Expr::Binary(expr_binary) => {
            make_float(&mut expr_binary.left);
            make_float(&mut expr_binary.right);
        }
        _ => (),
    }
}
