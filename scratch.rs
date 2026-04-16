use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Test { a: u32 }

fn main() {
    let t = Test { a: 42 };
    let ser = bincode::serialize(&t).unwrap();
    let des: Test = bincode::deserialize(&ser).unwrap();
    assert_eq!(t, des);

    let w_ser = wincode::serialize(&t).unwrap();
    let w_des: Test = wincode::deserialize(&w_ser).unwrap();
    assert_eq!(t, w_des);
    println!("wincode works!");
}
