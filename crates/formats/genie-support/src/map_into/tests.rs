use super::*;

#[test]
fn option_maps_into_the_wider_integer_and_none_stays_none() {
    let small: Option<u8> = Some(10);
    let wide: Option<u16> = small.map_into();
    assert_eq!(wide, Some(10));
    let none: Option<u16> = None::<u8>.map_into();
    assert_eq!(none, None);
}

#[test]
fn result_maps_its_value_into_the_wider_integer_and_keeps_its_error() {
    let value: Result<u16, &str> = Ok::<u8, &str>(10).map_into();
    assert_eq!(value, Ok(10));
    let error: Result<u16, &str> = Err::<u8, &str>("no value").map_into();
    assert_eq!(error, Err("no value"));
}

#[test]
fn map_into_is_an_extension_trait_of_option_and_result() {
    fn extended<Container: MapIntoExt<Target>, Target>(container: Container) -> Target {
        container.map_into()
    }
    assert_eq!(extended::<_, Option<u32>>(Some(7u16)), Some(7));
    assert_eq!(extended::<_, Result<u32, ()>>(Ok::<u16, ()>(7)), Ok(7));
}
