use rk_codegen::rk_tableaux;

#[rustfmt::skip]
rk_tableaux!(
    step_three_eigths,
    [0, 0.5, 0.5, 1],
    [
        [0],
        [0.5, 0],
        [0  , 0.5, 0,],
        [0  , 0  , 1, 0,],
    ],
    [1/6, 1/3, 1/3, 1/6]
);

fn main() {
    println!("Hello, world!");
    let f = |t, y| y * y * y + 10. * y - 15. * y * y + t * t;
    let out = step_three_eigths(0.001, 1.0, 0.0, f);
    dbg!(out);
}
