#[automatically_derived]
unsafe impl <'facet,'a,T:Facet<'a>+core::hash::Hash,const C:usize> ::facet::Facet<'facet>for E<'a,T,C>where T:std::fmt::Debug,[u8;
C]:std::fmt::Debug,'a:'facet,'facet:'a,T: ::facet::Facet<'facet>{
    const SHAPE: &'static::facet::Shape =  &const  {
        #[repr(C)]
        struct __ShadowE_Tuple<'facet,'a,T:Facet<'a>+core::hash::Hash,const C:usize>where T:std::fmt::Debug,[u8;
        C]:std::fmt::Debug,'a:'facet,'facet:'a,T: ::facet::Facet<'facet>{
            _discriminant:u8,___phantom: ::core::marker::PhantomData<(*mut &'facet(),'a,T,C)>,_0:T,_1:core::marker::PhantomData<&'a[u8;
            C]>
        }
        let __facet_variants: &'static[::facet::Variant] =  &const  {
            [::facet::Variant::builder().name("Unit").discriminant(0).fields(::facet::Struct::builder().unit().build()).build(),{
                let fields: &'static[::facet::Field] =  &const  {
                    [::facet::Field::builder().name("_0").shape(|| ::facet::shape_of(&|s: &__ShadowE_Tuple<'a,T,C>| &s._0)).offset(::core::mem::offset_of!(__ShadowE_Tuple<'a,T,C>,_0)).flags(::facet::FieldFlags::EMPTY).attributes(&const  {
                        []
                    }).build(), ::facet::Field::builder().name("_1").shape(|| ::facet::shape_of(&|s: &__ShadowE_Tuple<'a,T,C>| &s._1)).offset(::core::mem::offset_of!(__ShadowE_Tuple<'a,T,C>,_1)).flags(::facet::FieldFlags::EMPTY).attributes(&const  {
                        []
                    }).build()]
                };
                ::facet::Variant::builder().name("Tuple").discriminant(1).fields(::facet::Struct::builder().tuple().fields(fields).build()).build()
            }]
        };
        ::facet::Shape::builder().id(::facet::ConstTypeId::of::<Self>()).layout(::core::alloc::Layout::new::<Self>()).type_params(&[::facet::TypeParam {
            name:"T",shape: || <T as ::facet::Facet>::SHAPE
        }]).vtable(::facet::value_vtable!(Self, |f,_opts| ::core::fmt::Write::write_str(f,"E"))).def(::facet::Def::Enum(::facet::EnumDef::builder().variants(__facet_variants).repr(::facet::EnumRepr::U8).build())).build()
    };
}