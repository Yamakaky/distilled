#[distilled_derive::distilled]
pub fn proc_add(items: Vec<u8>) -> u8 {
    items.iter().sum::<u8>() + 1
}
