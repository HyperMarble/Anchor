include!("zero_to_python_parts/value_transforms.rs");
include!("zero_to_python_parts/line_transforms.rs");
include!("zero_to_python_parts/transpile.rs");
include!("zero_to_python_parts/main_a.rs");
include!("zero_to_python_parts/main_b.rs");
include!("zero_to_python_parts/main_c.rs");

fn main() {
    run_main_a();
    run_main_b();
    run_main_c();
}
