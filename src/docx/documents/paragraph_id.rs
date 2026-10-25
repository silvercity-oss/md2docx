use std::cell::Cell;

thread_local! {
    static PARA_ID: Cell<u32> = const { Cell::new(1) };
}

pub fn generate_para_id() -> String {
    let id = PARA_ID.with(|next| {
        let id = next.get();
        next.set(id.checked_add(1).expect("paragraph ID space exhausted"));
        id
    });
    format!("{id:08x}")
}

pub fn reset_para_id() {
    PARA_ID.set(1);
}
