/// Helper trait to map a container of `T` to a container of some type `F` that implements
/// `From<T>`.
///
/// For example, `Result<T>` to `Result<From<T>>`, or `Option<T>` to `Option<From<T>>`.
pub trait MapInto<T> {
    /// Map the contained `T` into a different type.
    ///
    /// Essentially, `.map_into()` is the same as doing `.map(|val| val.into())`.
    ///
    /// **Example**
    ///
    /// ```rust
    /// use genie_support::MapInto;
    ///
    /// let small: Option<u8> = Some(10);
    /// let wide: Option<u16> = small.map_into();
    /// assert_eq!(wide, Some(10u16));
    /// ```
    fn map_into(self) -> T;
}

impl<Source, Target, Error> MapInto<Result<Target, Error>> for Result<Source, Error>
where
    Target: From<Source>,
{
    #[inline]
    fn map_into(self) -> Result<Target, Error> {
        self.map(Into::into)
    }
}

impl<Source, Target> MapInto<Option<Target>> for Option<Source>
where
    Target: From<Source>,
{
    #[inline]
    fn map_into(self) -> Option<Target> {
        self.map(Into::into)
    }
}
