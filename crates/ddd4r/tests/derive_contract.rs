//! Compile and behavior contracts for public derive macros.

use ddd4r::core::domain::DomainModel as _;
use ddd4r::{DomainModel, Entity, ValueObject};

#[derive(Debug, DomainModel, Entity)]
struct Customer {
    #[ddd4r(id)]
    customer_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, ValueObject)]
struct Money(u64);

fn assert_value_object<T: ddd4r::core::domain::ValueObject>() {}

#[test]
fn derives_domain_identity_and_markers() {
    let customer = Customer {
        customer_id: "C-1".to_owned(),
    };
    assert_eq!(customer.id(), "C-1");
    assert_value_object::<Money>();
    assert_eq!(Money(10), Money(10).clone());
}
