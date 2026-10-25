/// Macro to delegate methods to a specified field
///
/// This macro generates methods that delegate to methods on a field of the struct.
/// Each delegated method will call the corresponding method on the specified field
/// and return `self` for method chaining.
///
macro_rules! delegate_to_field {
    ($field:ident => $($method:ident($($param:ident: $param_type:ty),*) -> Self),* $(,)?) => {
        $(
            pub fn $method(mut self, $($param: $param_type),*) -> Self {
                self.$field = self.$field.$method($($param),*);
                self
            }
        )*
    };
}

pub(crate) use delegate_to_field;
