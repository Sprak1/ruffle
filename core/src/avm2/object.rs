//! AVM2 objects.

use crate::avm2::activation::Activation;
use crate::avm2::array::ArrayStorage;
use crate::avm2::bytearray::ByteArrayStorage;
use crate::avm2::class::Class;
use crate::avm2::domain::Domain;
use crate::avm2::events::{DispatchList, Event};
use crate::avm2::function::Executable;
use crate::avm2::names::{Multiname, Namespace, QName};
use crate::avm2::regexp::RegExp;
use crate::avm2::scope::ScopeChain;
use crate::avm2::traits::{Trait, TraitKind};
use crate::avm2::value::{Hint, Value};
use crate::avm2::vector::VectorStorage;
use crate::avm2::Error;
use crate::backend::audio::{SoundHandle, SoundInstanceHandle};
use crate::bitmap::bitmap_data::BitmapData;
use crate::display_object::DisplayObject;
use crate::html::TextFormat;
use crate::string::AvmString;
use gc_arena::{Collect, GcCell, MutationContext};
use ruffle_macros::enum_trait_object;
use smallvec::SmallVec;
use std::cell::{Ref, RefMut};
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

mod array_object;
mod bitmapdata_object;
mod bytearray_object;
mod class_object;
mod date_object;
mod dictionary_object;
mod dispatch_object;
mod domain_object;
mod event_object;
mod function_object;
mod loaderinfo_object;
mod namespace_object;
mod primitive_object;
mod proxy_object;
mod qname_object;
mod regexp_object;
mod script_object;
mod sound_object;
mod soundchannel_object;
mod stage_object;
mod textformat_object;
mod vector_object;
mod xml_object;

pub use crate::avm2::object::array_object::{array_allocator, ArrayObject};
pub use crate::avm2::object::bitmapdata_object::{bitmapdata_allocator, BitmapDataObject};
pub use crate::avm2::object::bytearray_object::{bytearray_allocator, ByteArrayObject};
pub use crate::avm2::object::class_object::ClassObject;
pub use crate::avm2::object::date_object::{date_allocator, DateObject};
pub use crate::avm2::object::dictionary_object::{dictionary_allocator, DictionaryObject};
pub use crate::avm2::object::dispatch_object::DispatchObject;
pub use crate::avm2::object::domain_object::{appdomain_allocator, DomainObject};
pub use crate::avm2::object::event_object::{event_allocator, EventObject};
pub use crate::avm2::object::function_object::FunctionObject;
pub use crate::avm2::object::loaderinfo_object::{
    loaderinfo_allocator, LoaderInfoObject, LoaderStream,
};
pub use crate::avm2::object::namespace_object::{namespace_allocator, NamespaceObject};
pub use crate::avm2::object::primitive_object::{primitive_allocator, PrimitiveObject};
pub use crate::avm2::object::proxy_object::{proxy_allocator, ProxyObject};
pub use crate::avm2::object::qname_object::{qname_allocator, QNameObject};
pub use crate::avm2::object::regexp_object::{regexp_allocator, RegExpObject};
pub use crate::avm2::object::script_object::{ScriptObject, ScriptObjectData};
pub use crate::avm2::object::sound_object::{sound_allocator, SoundObject};
pub use crate::avm2::object::soundchannel_object::{soundchannel_allocator, SoundChannelObject};
pub use crate::avm2::object::stage_object::{stage_allocator, StageObject};
pub use crate::avm2::object::textformat_object::{textformat_allocator, TextFormatObject};
pub use crate::avm2::object::vector_object::{vector_allocator, VectorObject};
pub use crate::avm2::object::xml_object::{xml_allocator, XmlObject};

/// Represents an object that can be directly interacted with by the AVM2
/// runtime.
#[enum_trait_object(
    #[allow(clippy::enum_variant_names)]
    #[derive(Clone, Collect, Debug, Copy)]
    #[collect(no_drop)]
    pub enum Object<'gc> {
        ScriptObject(ScriptObject<'gc>),
        FunctionObject(FunctionObject<'gc>),
        PrimitiveObject(PrimitiveObject<'gc>),
        NamespaceObject(NamespaceObject<'gc>),
        ArrayObject(ArrayObject<'gc>),
        StageObject(StageObject<'gc>),
        DomainObject(DomainObject<'gc>),
        EventObject(EventObject<'gc>),
        DispatchObject(DispatchObject<'gc>),
        XmlObject(XmlObject<'gc>),
        RegExpObject(RegExpObject<'gc>),
        ByteArrayObject(ByteArrayObject<'gc>),
        LoaderInfoObject(LoaderInfoObject<'gc>),
        ClassObject(ClassObject<'gc>),
        VectorObject(VectorObject<'gc>),
        SoundObject(SoundObject<'gc>),
        SoundChannelObject(SoundChannelObject<'gc>),
        BitmapDataObject(BitmapDataObject<'gc>),
        DateObject(DateObject<'gc>),
        DictionaryObject(DictionaryObject<'gc>),
        QNameObject(QNameObject<'gc>),
        TextFormatObject(TextFormatObject<'gc>),
        ProxyObject(ProxyObject<'gc>),
    }
)]
pub trait TObject<'gc>: 'gc + Collect + Debug + Into<Object<'gc>> + Clone + Copy {
    /// Get the base of this object.
    /// Any trait method implementations that were not overrided will forward the call to this instead.
    fn base(&self) -> Ref<ScriptObjectData<'gc>>;
    fn base_mut(&self, mc: MutationContext<'gc, '_>) -> RefMut<ScriptObjectData<'gc>>;

    /// Retrieve a property by QName, after multiname resolution, prototype
    /// lookups, and all other considerations have been taken.
    ///
    /// This required method is only intended to be called by other TObject
    /// methods.
    fn get_property_local(
        self,
        receiver: Object<'gc>,
        name: QName<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error> {
        let base = self.base();
        let rv = base.get_property_local(receiver, name, activation)?;

        drop(base);

        rv.resolve(activation)
    }

    /// Retrieve a property that does not exist.
    ///
    /// By default, this returns an error for sealed classes, and `undefined`
    /// for dynamic ones. Objects that have particular alternative behavior for
    /// undefined values may substitute their own implementation here without
    /// disturbing the rest of `getproperty`'s implementation.
    fn get_property_undef(
        self,
        _receiver: Object<'gc>,
        multiname: &Multiname<'gc>,
        _activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error> {
        // Special case: Unresolvable properties on dynamic classes are treated
        // as dynamic properties that have not yet been set, and yield
        // `undefined`
        if !self
            .instance_of_class_definition()
            .map(|c| c.read().is_sealed())
            .unwrap_or(false)
        {
            return Ok(Value::Undefined);
        }

        return Err(format!("Cannot get undefined property {:?}", multiname.local_name()).into());
    }

    /// Retrieve a property by Multiname lookup.
    ///
    /// This corresponds directly to the AVM2 operation `getproperty`, with the
    /// exception that it does not special-case object lookups on dictionary
    /// structured objects.
    #[allow(unused_mut)] //Not unused.
    fn get_property(
        mut self,
        receiver: Object<'gc>,
        multiname: &Multiname<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error> {
        let name = self.resolve_multiname(multiname)?;

        if name.is_none() {
            return self.get_property_undef(receiver, multiname, activation);
        }

        // At this point, the name must be a valid QName.
        let name = name.unwrap();
        if !self.base().has_own_instantiated_property(name) {
            // Initialize lazy-bound methods at this point in time.
            if let Some(class) = self.instance_of() {
                if let Some((bound_method, disp_id)) =
                    class.bound_instance_method(activation, receiver, name)?
                {
                    self.install_method(activation.context.gc_context, name, disp_id, bound_method);

                    return Ok(bound_method.into());
                }
            }

            // Class methods are also lazy-bound.
            if let Some(class) = self.as_class_object() {
                if let Some((bound_method, disp_id)) = class.bound_class_method(activation, name)? {
                    self.install_method(activation.context.gc_context, name, disp_id, bound_method);

                    return Ok(bound_method.into());
                }
            }
        }

        let is_set_only = self.base().has_own_virtual_set_only_property(name);

        if self.has_own_property(name)? && !is_set_only {
            return self.get_property_local(receiver, name, activation);
        }

        if let Some(proto) = self.proto() {
            return proto.get_property(receiver, multiname, activation);
        }

        Ok(Value::Undefined)
    }

    /// Set a property by QName, after multiname resolution and all other
    /// considerations have been taken.
    ///
    /// This required method is only intended to be called by other TObject
    /// methods.
    fn set_property_local(
        self,
        receiver: Object<'gc>,
        name: QName<'gc>,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<(), Error> {
        let mut base = self.base_mut(activation.context.gc_context);

        let rv = base.set_property_local(receiver, name, value, activation)?;

        drop(base);

        rv.resolve(activation)?;

        Ok(())
    }

    /// Qualify the name of a property that does not exist, in the context of
    /// setting a property.
    ///
    /// By default, this returns an error for sealed classes, and a name in the
    /// public namespace for dynamic ones. Objects that have particular
    /// alternative behavior for undefined values may substitute their own
    /// implementation here without disturbing the rest of `setproperty`'s
    /// implementation.
    ///
    /// This function typically returns a `QName` or an error. Returning `None`
    /// indicates that the object handled the `setproperty` without needing to
    /// store anything.
    fn set_property_undef(
        &mut self,
        _receiver: Object<'gc>,
        multiname: &Multiname<'gc>,
        _value: Value<'gc>,
        _activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Option<QName<'gc>>, Error> {
        // Special case: Unresolvable properties on dynamic classes are treated
        // as initializing a new dynamic property on namespace Public("").
        if !self
            .instance_of_class_definition()
            .map(|c| c.read().is_sealed())
            .unwrap_or(false)
        {
            let local_name: Result<AvmString<'gc>, Error> = multiname
                .local_name()
                .ok_or_else(|| "Cannot set undefined property using any name".into());
            Ok(Some(QName::dynamic_name(local_name?)))
        } else {
            Err(format!("Cannot set undefined property {:?}", multiname.local_name()).into())
        }
    }

    /// Set a property by Multiname lookup.
    ///
    /// This corresponds directly with the AVM2 operation `setproperty`, with
    /// the exception that it does not special-case object lookups on
    /// dictionary structured objects.
    fn set_property(
        &mut self,
        receiver: Object<'gc>,
        multiname: &Multiname<'gc>,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<(), Error> {
        let mut name = self.resolve_multiname(multiname)?;

        if name.is_none() {
            name = self.set_property_undef(receiver, multiname, value, activation)?;
        }

        if name.is_none() {
            return Ok(());
        }

        // At this point, name resolution should have completed.
        let name = name.unwrap();

        // Reject attempts to overwrite lazy-bound methods before they have
        // been bound.
        if let Some(class) = self.instance_of() {
            if class.instance_method(name)?.is_some() {
                return Err(format!(
                    "Cannot overwrite read-only property {:?}",
                    name.local_name()
                )
                .into());
            }
        }

        if let Some(class) = self.as_class_object() {
            if class.class_method(name)?.is_some() {
                return Err(format!(
                    "Cannot overwrite read-only property {:?}",
                    name.local_name()
                )
                .into());
            }
        }

        self.set_property_local(receiver, name, value, activation)
    }

    /// Initialize a property by QName, after multiname resolution and all
    /// other considerations have been taken.
    ///
    /// This required method is only intended to be called by other TObject
    /// methods.
    fn init_property_local(
        self,
        receiver: Object<'gc>,
        name: QName<'gc>,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<(), Error> {
        let mut base = self.base_mut(activation.context.gc_context);
        let rv = base.init_property_local(receiver, name, value, activation)?;

        drop(base);

        rv.resolve(activation)?;

        Ok(())
    }

    /// Initialize a property by Multiname lookup.
    ///
    /// This corresponds directly with the AVM2 operation `initproperty`.
    fn init_property(
        &mut self,
        receiver: Object<'gc>,
        multiname: &Multiname<'gc>,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<(), Error> {
        let mut name = self.resolve_multiname(multiname)?;

        if name.is_none() {
            name = self.set_property_undef(receiver, multiname, value, activation)?;
        }

        if name.is_none() {
            return Ok(());
        }

        // At this point, name resolution should have completed.
        let name = name.unwrap();

        // Reject attempts to overwrite lazy-bound methods before they have
        // been bound.
        if let Some(class) = self.instance_of() {
            if class.instance_method(name)?.is_some() {
                return Err(format!(
                    "Cannot overwrite read-only property {:?}",
                    name.local_name()
                )
                .into());
            }
        }

        if let Some(class) = self.as_class_object() {
            if class.class_method(name)?.is_some() {
                return Err(format!(
                    "Cannot overwrite read-only property {:?}",
                    name.local_name()
                )
                .into());
            }
        }

        self.init_property_local(receiver, name, value, activation)
    }

    /// Call a named property that does not exist.
    ///
    /// By default, this returns an error. Objects that have particular
    /// alternative behavior for calling undefined properties may substitute
    /// their own implementation here without disturbing the rest of
    /// `callproperty`'s implementation.
    fn call_property_undef(
        self,
        multiname: &Multiname<'gc>,
        _arguments: &[Value<'gc>],
        _activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error> {
        Err(format!(
            "Attempted to call undefined property {:?}",
            multiname.local_name()
        )
        .into())
    }

    /// Call a named property on the object.
    ///
    /// This corresponds directly to the `callproperty` operation in AVM2.
    fn call_property(
        self,
        multiname: &Multiname<'gc>,
        arguments: &[Value<'gc>],
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error> {
        let name = self.resolve_multiname(multiname)?;
        if name.is_none() {
            return self.call_property_undef(multiname, arguments, activation);
        }

        let name = name.unwrap();

        if let Some(class) = self.instance_of() {
            if let Some((superclass, method_trait)) = class.instance_method(name)? {
                let method = method_trait.as_method().unwrap();
                if !method.needs_arguments_object() {
                    let scope = class.instance_scope();

                    return Executable::from_method(method, scope, None, Some(superclass)).exec(
                        Some(self.into()),
                        arguments,
                        activation,
                        superclass.into(), //Deliberately invalid.
                    );
                }
            }
        }

        if let Some(class) = self.as_class_object() {
            if let Some(method_trait) = class.class_method(name)? {
                let method = method_trait.as_method().unwrap();
                if !method.needs_arguments_object() {
                    let scope = class.class_scope();

                    return Executable::from_method(method, scope, None, Some(class)).exec(
                        Some(self.into()),
                        arguments,
                        activation,
                        class.into(), //Deliberately invalid.
                    );
                }
            }
        }

        let function = self
            .get_property(self.into(), multiname, activation)?
            .coerce_to_object(activation);
        if function.is_err() {
            return Err(format!(
                "Attempted to call undefined property {:?}",
                multiname.local_name()
            )
            .into());
        }

        function
            .unwrap()
            .call(Some(self.into()), arguments, activation)
    }

    /// Retrieve a slot by its index.
    fn get_slot(self, id: u32) -> Result<Value<'gc>, Error> {
        let base = self.base();

        base.get_slot(id)
    }

    /// Set a slot by its index.
    fn set_slot(
        self,
        id: u32,
        value: Value<'gc>,
        mc: MutationContext<'gc, '_>,
    ) -> Result<(), Error> {
        let mut base = self.base_mut(mc);

        base.set_slot(id, value, mc)
    }

    /// Initialize a slot by its index.
    fn init_slot(
        self,
        id: u32,
        value: Value<'gc>,
        mc: MutationContext<'gc, '_>,
    ) -> Result<(), Error> {
        let mut base = self.base_mut(mc);

        base.init_slot(id, value, mc)
    }

    /// Call a method by its index.
    ///
    /// This directly corresponds with the AVM2 operation `callmethod`.
    #[allow(unused_mut)] //Not unused.
    fn call_method(
        mut self,
        id: u32,
        arguments: &[Value<'gc>],
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error> {
        if self.base().get_method(id).is_none() {
            if let Some(class) = self.instance_of() {
                if let Some((bound_method, name)) =
                    class.bound_instance_method_by_id(activation, self.into(), id)?
                {
                    self.install_method(activation.context.gc_context, name, id, bound_method);
                }
            }

            if let Some(class) = self.as_class_object() {
                if let Some((bound_method, name)) =
                    class.bound_class_method_by_id(activation, id)?
                {
                    self.install_method(activation.context.gc_context, name, id, bound_method);
                }
            }
        }

        if let Some(method_object) = self.base().get_method(id) {
            return method_object.call(Some(self.into()), arguments, activation);
        }

        Err(format!("Cannot call unknown method id {}", id).into())
    }

    /// Resolve a multiname into a single QName, if any of the namespaces
    /// match.
    ///
    /// If multiple namespaces could match, this yields an error.
    fn resolve_multiname(self, multiname: &Multiname<'gc>) -> Result<Option<QName<'gc>>, Error> {
        let matching_set = if let Some(local_name) = multiname.local_name() {
            self.resolve_ns(local_name)?
        } else {
            smallvec![]
        };

        if let Some(name) = multiname.local_name() {
            for ns in matching_set.iter() {
                if multiname.namespace_set().any(|n| n == ns) {
                    return Ok(Some(QName::new(*ns, name)));
                }
            }
        }

        if multiname.namespace_set().any(|n| *n == Namespace::Any) {
            return Ok(matching_set
                .first()
                .map(|ns| QName::new(*ns, multiname.local_name().unwrap())));
        }

        if let Some(proto) = self.proto() {
            return proto.resolve_multiname(multiname);
        }

        Ok(None)
    }

    /// Given a local name, list all of the namespaces that have properties
    /// with that local name.
    ///
    /// None of the returned `Namespace`s can be `Namespace::Any`, as this
    /// function is also used to resolve such names.
    ///
    /// Trait names will be resolved on class objects and object instances, but
    /// not prototypes. If you want to search a prototype's provided traits you
    /// must walk the prototype chain using `resolve_any_trait`.
    fn resolve_ns(
        self,
        local_name: AvmString<'gc>,
    ) -> Result<SmallVec<[Namespace<'gc>; 1]>, Error> {
        let base = self.base();

        base.resolve_ns(local_name)
    }

    /// Implements the `in` opcode and AS3 operator.
    ///
    /// By default, this just calls `has_property`, but may be overridden by
    /// other object types to change the behavior of the `in` operator only.
    fn has_property_via_in(
        self,
        _activation: &mut Activation<'_, 'gc, '_>,
        name: QName<'gc>,
    ) -> Result<bool, Error> {
        self.has_property(name)
    }

    /// Indicates whether or not a property exists on an object.
    fn has_property(self, name: QName<'gc>) -> Result<bool, Error> {
        if self.has_own_property(name)? {
            Ok(true)
        } else if let Some(proto) = self.proto() {
            Ok(proto.has_own_property(name)?)
        } else {
            Ok(false)
        }
    }

    /// Indicates whether or not a property or trait exists on an object and is
    /// not part of the prototype chain.
    fn has_own_property(self, name: QName<'gc>) -> Result<bool, Error> {
        let base = self.base();

        base.has_own_property(name)
    }

    /// Returns true if an object has one or more traits of a given name.
    fn has_trait(self, name: QName<'gc>) -> Result<bool, Error> {
        let base = self.base();

        base.has_trait(name)
    }

    /// Indicates whether or not a property is overwritable.
    fn is_property_overwritable(
        self,
        _gc_context: MutationContext<'gc, '_>,
        name: QName<'gc>,
    ) -> bool {
        let base = self.base();

        base.is_property_overwritable(name)
    }

    /// Delete a property by QName, after multiname resolution and all other
    /// considerations have been taken.
    ///
    /// This required method is only intended to be called by other TObject
    /// methods.
    fn delete_property_local(
        &self,
        gc_context: MutationContext<'gc, '_>,
        name: QName<'gc>,
    ) -> Result<bool, Error> {
        let mut base = self.base_mut(gc_context);

        Ok(base.delete_property(name))
    }

    /// Delete a property that does not exist.
    ///
    /// By default, undefined property deletion succeeds for dynamic classes,
    /// and fails for sealed ones. Objects that have particular alternative
    /// behavior for undefined values may substitute their own implementation
    /// here without disturbing the rest of `deleteproperty`'s implementation.
    fn delete_property_undef(
        &self,
        _activation: &mut Activation<'_, 'gc, '_>,
        _multiname: &Multiname<'gc>,
    ) -> Result<bool, Error> {
        // Unknown properties on a dynamic class delete successfully.
        return Ok(!self
            .instance_of_class_definition()
            .map(|c| c.read().is_sealed())
            .unwrap_or(false));
    }

    /// Delete a named property from the object.
    ///
    /// Returns false if the property cannot be deleted.
    fn delete_property(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        multiname: &Multiname<'gc>,
    ) -> Result<bool, Error> {
        let name = self.resolve_multiname(multiname)?;

        if name.is_none() {
            return self.delete_property_undef(activation, multiname);
        }

        //At this point, the name should be known.
        let name = name.unwrap();

        // Reject attempts to delete lazy-bound methods before they have
        // been bound.
        if !self.base().has_own_instantiated_property(name) {
            if let Some(class) = self.instance_of() {
                if class
                    .instance_method(name)
                    .map(|t| t.is_some())
                    .unwrap_or(false)
                {
                    return Ok(false);
                }
            }

            if let Some(class) = self.as_class_object() {
                if class
                    .class_method(name)
                    .map(|t| t.is_some())
                    .unwrap_or(false)
                {
                    return Ok(false);
                }
            }
        }

        self.delete_property_local(activation.context.gc_context, name)
    }

    /// Retrieve the `__proto__` of a given object.
    ///
    /// The proto is another object used to resolve methods across a class of
    /// multiple objects. It should also be accessible as `__proto__` from
    /// `get`.
    fn proto(&self) -> Option<Object<'gc>> {
        let base = self.base();

        base.proto()
    }

    /// Change the `__proto__` on this object.
    ///
    /// This method primarily exists so that the global scope that player
    /// globals loads into can be created before its superclasses are. It
    /// should be used sparingly, if at all.
    fn set_proto(self, mc: MutationContext<'gc, '_>, proto: Object<'gc>) {
        let mut base = self.base_mut(mc);

        base.set_proto(proto)
    }

    /// Get the next enumerant index in enumerant space.
    ///
    /// Every object maintains a list of enumerants - key/value pairs indexed
    /// by arbitrary integers. This function allows clients to iterate through
    /// those indexing integers. Indexing starts with zero, and then the caller
    /// repeatedly hands prior results back into this function until it returns
    /// None.
    ///
    /// Repeated calls to this function with prior return values must
    /// eventually return `None`. Furthermore, returning `0`, while valid, is
    /// treated by AVM2 code as signalling `None`.
    fn get_next_enumerant(
        self,
        last_index: u32,
        _activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Option<u32>, Error> {
        let base = self.base();

        Ok(base.get_next_enumerant(last_index))
    }

    /// Retrieve a given enumerable name by index.
    ///
    /// Enumerants are listed by index, starting from zero and iterated via
    /// `get_next_enumerant`. Only enumerants returned by that function are
    /// valid here. A value of `None` indicates that no enumerant with that
    /// index exists.
    fn get_enumerant_name(
        self,
        index: u32,
        _activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error> {
        let base = self.base();

        Ok(base.get_enumerant_name(index).unwrap_or(Value::Undefined))
    }

    /// Retrieve a given enumerable value by index.
    ///
    /// This default implementation of value retrieval assumes that the names
    /// of enumerants are also valid local names in the public namespace.
    fn get_enumerant_value(
        self,
        index: u32,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error> {
        let name = self
            .get_enumerant_name(index, activation)?
            .coerce_to_string(activation)?;
        self.get_property(self.into(), &QName::dynamic_name(name).into(), activation)
    }

    /// Determine if a property is currently enumerable.
    ///
    /// Properties that do not exist are also not enumerable.
    fn property_is_enumerable(&self, name: QName<'gc>) -> bool {
        let base = self.base();

        base.property_is_enumerable(name)
    }

    /// Mark a dynamic property on this object as enumerable.
    fn set_local_property_is_enumerable(
        &self,
        mc: MutationContext<'gc, '_>,
        name: QName<'gc>,
        is_enumerable: bool,
    ) -> Result<(), Error> {
        let mut base = self.base_mut(mc);

        base.set_local_property_is_enumerable(name, is_enumerable)
    }

    /// Install a method (or any other non-slot value) on an object.
    fn install_method(
        &mut self,
        mc: MutationContext<'gc, '_>,
        name: QName<'gc>,
        disp_id: u32,
        function: Object<'gc>,
    ) {
        let mut base = self.base_mut(mc);

        base.install_method(name, disp_id, function)
    }

    /// Install a getter method on an object property.
    fn install_getter(
        &mut self,
        mc: MutationContext<'gc, '_>,
        name: QName<'gc>,
        disp_id: u32,
        function: Object<'gc>,
    ) -> Result<(), Error> {
        let mut base = self.base_mut(mc);

        base.install_getter(name, disp_id, function)
    }

    /// Install a setter method on an object property.
    fn install_setter(
        &mut self,
        mc: MutationContext<'gc, '_>,
        name: QName<'gc>,
        disp_id: u32,
        function: Object<'gc>,
    ) -> Result<(), Error> {
        let mut base = self.base_mut(mc);

        base.install_setter(name, disp_id, function)
    }

    /// Install a dynamic or built-in value property on an object.
    fn install_dynamic_property(
        &mut self,
        mc: MutationContext<'gc, '_>,
        name: QName<'gc>,
        value: Value<'gc>,
    ) -> Result<(), Error> {
        let mut base = self.base_mut(mc);

        base.install_dynamic_property(name, value)
    }

    /// Install a slot on an object property.
    fn install_slot(
        &mut self,
        mc: MutationContext<'gc, '_>,
        name: QName<'gc>,
        id: u32,
        value: Value<'gc>,
    ) {
        let mut base = self.base_mut(mc);

        base.install_slot(name, id, value)
    }

    /// Install a const on an object property.
    fn install_const(
        &mut self,
        mc: MutationContext<'gc, '_>,
        name: QName<'gc>,
        id: u32,
        value: Value<'gc>,
    ) {
        let mut base = self.base_mut(mc);

        base.install_const(name, id, value)
    }

    /// Install all instance traits provided by a class.
    ///
    /// This method will also install superclass instance traits first. By
    /// calling this method with the lowest class in the chain, you will ensure
    /// all instance traits are installed.
    ///
    /// Read the documentation for `install_trait` to learn more about exactly
    /// how traits are instantiated.
    fn install_instance_traits(
        &mut self,
        activation: &mut Activation<'_, 'gc, '_>,
        from_class_object: ClassObject<'gc>,
    ) -> Result<(), Error> {
        if let Some(superclass_object) = from_class_object.superclass_object() {
            self.install_instance_traits(activation, superclass_object)?;
        }

        let class = from_class_object.inner_class_definition();
        self.install_traits(
            activation,
            class.read().instance_traits(),
            from_class_object.instance_scope(),
            Some(from_class_object),
        )?;

        Ok(())
    }

    /// Install a list of traits into this object.
    ///
    /// This function should be called immediately after object allocation and
    /// before any constructors have a chance to run.
    ///
    /// Read the documentation for `install_trait` to learn more about exactly
    /// how traits are instantiated.
    fn install_traits(
        &mut self,
        activation: &mut Activation<'_, 'gc, '_>,
        traits: &[Trait<'gc>],
        scope: ScopeChain<'gc>,
        defining_class: Option<ClassObject<'gc>>,
    ) -> Result<(), Error> {
        for trait_entry in traits {
            self.install_trait(activation, trait_entry, scope, defining_class)?;
        }

        Ok(())
    }

    /// Install a single trait into this object.
    ///
    /// This function should be called immediately after object allocation and
    /// before any constructors have a chance to run. It should also only be
    /// called once per name and/or slot ID, as reinstalling a trait may unset
    /// already set properties.
    ///
    /// Class, function, and method traits are *not* instantiated at
    /// installation time. Instead, installing such traits merely installs a
    /// placeholder (such as an `undefined` const slot) until the trait is
    /// properly initialized.
    ///
    /// All traits that are instantiated at install time will be instantiated
    /// with this object's current scope stack and this object as a bound
    /// receiver.
    ///
    /// The value of the trait at the time of installation will be returned
    /// here, or `undefined` for classes, functions, and methods.
    ///
    /// No verification happens in `install_trait`. Instead, you must make sure
    /// to only install traits from validated classes.
    fn install_trait(
        &mut self,
        activation: &mut Activation<'_, 'gc, '_>,
        trait_entry: &Trait<'gc>,
        scope: ScopeChain<'gc>,
        defining_class: Option<ClassObject<'gc>>,
    ) -> Result<Value<'gc>, Error> {
        let receiver = (*self).into();
        let trait_name = trait_entry.name();

        avm_debug!(
            activation.avm2(),
            "Installing trait {:?} of kind {:?}",
            trait_name,
            trait_entry.kind()
        );

        match trait_entry.kind() {
            TraitKind::Slot {
                slot_id,
                default_value,
                ..
            } => {
                self.install_slot(
                    activation.context.gc_context,
                    trait_name,
                    *slot_id,
                    *default_value,
                );

                Ok(*default_value)
            }
            TraitKind::Method { .. } => Ok(Value::Undefined),
            TraitKind::Getter {
                disp_id, method, ..
            } => {
                let function = FunctionObject::from_method(
                    activation,
                    method.clone(),
                    scope,
                    Some(receiver),
                    defining_class,
                );
                self.install_getter(
                    activation.context.gc_context,
                    trait_name,
                    *disp_id,
                    function,
                )?;

                Ok(function.into())
            }
            TraitKind::Setter {
                disp_id, method, ..
            } => {
                let function = FunctionObject::from_method(
                    activation,
                    method.clone(),
                    scope,
                    Some(receiver),
                    defining_class,
                );
                self.install_setter(
                    activation.context.gc_context,
                    trait_name,
                    *disp_id,
                    function,
                )?;

                Ok(function.into())
            }
            TraitKind::Class { slot_id, .. } => {
                self.install_const(
                    activation.context.gc_context,
                    trait_name,
                    *slot_id,
                    Value::Undefined,
                );

                Ok(Value::Undefined)
            }
            TraitKind::Function { slot_id, .. } => {
                self.install_const(
                    activation.context.gc_context,
                    trait_name,
                    *slot_id,
                    Value::Undefined,
                );

                Ok(Value::Undefined)
            }
            TraitKind::Const {
                slot_id,
                default_value,
                ..
            } => {
                self.install_const(
                    activation.context.gc_context,
                    trait_name,
                    *slot_id,
                    *default_value,
                );

                Ok(*default_value)
            }
        }
    }

    /// Call the object.
    fn call(
        self,
        _reciever: Option<Object<'gc>>,
        _arguments: &[Value<'gc>],
        _activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error> {
        Err("Object is not callable".into())
    }

    /// Construct a Class or Function and return an instance of it.
    ///
    /// As the first step in object construction, the `construct` method is
    /// called on the class object to produce an instance of that class. The
    /// constructor is then expected to perform the following steps, in order:
    ///
    /// 1. Allocate the instance object. For ES4 classes, the class's instance
    /// allocator is used to allocate the object. ES3-style classes use the
    /// prototype to derive instances.
    /// 2. Associate the instance object with the class's explicit `prototype`.
    /// 3. If the class has instance traits, install them at this time.
    /// 4. Call the constructor method with the newly-allocated object as
    /// reciever. For ES3 classes, this is just the function's associated
    /// method.
    /// 5. Yield the allocated object. (The return values of constructors are
    /// ignored.)
    fn construct(
        self,
        _activation: &mut Activation<'_, 'gc, '_>,
        _args: &[Value<'gc>],
    ) -> Result<Object<'gc>, Error> {
        Err("Object is not constructable".into())
    }

    /// Construct a property of this object by Multiname lookup.
    ///
    /// This corresponds directly to the AVM2 operation `constructprop`.
    fn construct_prop(
        self,
        multiname: &Multiname<'gc>,
        args: &[Value<'gc>],
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Object<'gc>, Error> {
        let ctor = self
            .get_property(self.into(), multiname, activation)?
            .coerce_to_object(activation)?;

        ctor.construct(activation, args)
    }

    /// Construct a host object prototype of some kind and return it.
    ///
    /// This is called specifically to allocate old-style ES3 instances. The
    /// returned object should have no properties upon it.
    fn derive(&self, activation: &mut Activation<'_, 'gc, '_>) -> Result<Object<'gc>, Error>;

    /// Construct a parameterization of this particular type and return it.
    ///
    /// This is called specifically to parameterize generic types, of which
    /// only one exists: `Vector`. When `Vector` is applied with a given
    /// parameter, a new type is returned which can be used to construct
    /// `Vector`s of that type.
    ///
    /// If the object is not a parameterized type, this yields an error. In
    /// practice, this means only `Vector` can use this method. Parameters must
    /// be class objects or `null`, which indicates any type.
    ///
    /// When a given type is parameterized with the same parameters multiple
    /// times, each application must return the same object. This is because
    /// each application has a separate prototype that accepts dynamic
    /// parameters.
    fn apply(
        &self,
        _activation: &mut Activation<'_, 'gc, '_>,
        _params: &[Value<'gc>],
    ) -> Result<ClassObject<'gc>, Error> {
        Err("Not a parameterized type".into())
    }

    /// Determine the type of primitive coercion this object would prefer, in
    /// the case that there is no obvious reason to prefer one type over the
    /// other.
    ///
    /// All native ECMAScript objects prefer numerical coercions, except `Date`,
    /// which wants string coercions.
    fn default_hint(&self) -> Hint {
        Hint::Number
    }

    /// Implement the result of calling `Object.prototype.toString` on this
    /// object class.
    ///
    /// `toString` is a method used to request an object be coerced to a string
    /// value. The default implementation is stored here. User-specified string
    /// coercions happen by defining `toString` in a downstream class or
    /// prototype; this is then picked up by the VM runtime when doing
    /// coercions.
    fn to_string(&self, mc: MutationContext<'gc, '_>) -> Result<Value<'gc>, Error> {
        let class_name = self
            .instance_of_class_definition()
            .map(|c| c.read().name().local_name())
            .unwrap_or_else(|| "Object".into());

        Ok(AvmString::new_utf8(mc, format!("[object {}]", class_name)).into())
    }

    /// Implement the result of calling `Object.prototype.toLocaleString` on this
    /// object class.
    ///
    /// `toLocaleString` is a method used to request an object be coerced to a
    /// locale-dependent string value. The default implementation appears to
    /// generate a debug-style string based on the name of the class this
    /// object is, in the format of `[object Class]` (where `Class` is the name
    /// of the class that created this object).
    fn to_locale_string(&self, mc: MutationContext<'gc, '_>) -> Result<Value<'gc>, Error> {
        let class_name = self
            .instance_of_class_definition()
            .map(|c| c.read().name().local_name())
            .unwrap_or_else(|| "Object".into());

        Ok(AvmString::new_utf8(mc, format!("[object {}]", class_name)).into())
    }

    /// Implement the result of calling `Object.prototype.valueOf` on this
    /// object class.
    ///
    /// `valueOf` is a method used to request an object be coerced to a
    /// primitive value. Typically, this would be a number of some kind.
    fn value_of(&self, mc: MutationContext<'gc, '_>) -> Result<Value<'gc>, Error>;

    /// Determine if this object is an instance of a given type.
    ///
    /// This uses the ES3 definition of instance, which walks the prototype
    /// chain. For the ES4 definition of instance, use `is_of_type`, which uses
    /// the class object chain and accounts for interfaces.
    ///
    /// The given object should be the class object for the given type we are
    /// checking against this object. Its prototype will be extracted and
    /// searched in the prototype chain of this object.
    fn is_instance_of(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        class: Object<'gc>,
    ) -> Result<bool, Error> {
        let type_proto = class
            .get_property(class, &QName::dynamic_name("prototype").into(), activation)?
            .coerce_to_object(activation)?;

        self.has_prototype_in_chain(type_proto)
    }

    /// Determine if this object has a given prototype in its prototype chain.
    ///
    /// The given object `type_proto` should be the prototype we are checking
    /// against this object.
    fn has_prototype_in_chain(&self, type_proto: Object<'gc>) -> Result<bool, Error> {
        let mut my_proto = self.proto();

        //TODO: Is it a verification error to do `obj instanceof bare_object`?
        while let Some(proto) = my_proto {
            if Object::ptr_eq(proto, type_proto) {
                return Ok(true);
            }

            my_proto = proto.proto()
        }

        Ok(false)
    }

    /// Determine if this object is an instance of a given type.
    ///
    /// This uses the ES4 definition of instance, which walks the class object
    /// chain and accounts for interfaces. For the ES3 definition of instance,
    /// use `is_instance_of`, which uses the prototype chain.
    ///
    /// The given object should be the class object for the given type we are
    /// checking against this object.
    fn is_of_type(
        &self,
        test_class: ClassObject<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<bool, Error> {
        let my_class = self.instance_of();

        // ES3 objects are not class instances but are still treated as
        // instances of Object, which is an ES4 class.
        if my_class.is_none() && Object::ptr_eq(test_class, activation.avm2().classes().object) {
            Ok(true)
        } else if let Some(my_class) = my_class {
            my_class.has_class_in_chain(test_class, activation)
        } else {
            Ok(false)
        }
    }

    /// Get a raw pointer value for this object.
    fn as_ptr(&self) -> *const ObjectPtr;

    /// Get this object's class, if it has one.
    fn instance_of(&self) -> Option<ClassObject<'gc>> {
        let base = self.base();

        base.instance_of()
    }

    /// Get this object's class's `Class`, if it has one.
    fn instance_of_class_definition(&self) -> Option<GcCell<'gc, Class<'gc>>> {
        self.instance_of().map(|cls| cls.inner_class_definition())
    }

    fn set_instance_of(&self, mc: MutationContext<'gc, '_>, instance_of: ClassObject<'gc>) {
        let mut base = self.base_mut(mc);

        base.set_instance_of(instance_of);
    }

    /// Try to corece this object into a `ClassObject`.
    fn as_class_object(&self) -> Option<ClassObject<'gc>> {
        None
    }

    /// Get this object's `Executable`, if it has one.
    fn as_executable(&self) -> Option<Ref<Executable<'gc>>> {
        None
    }

    /// Unwrap this object's `Namespace`, if the object is a boxed namespace.
    fn as_namespace(&self) -> Option<Ref<Namespace<'gc>>> {
        None
    }

    /// Unwrap this object as a `QNameObject`
    fn as_qname_object(self) -> Option<QNameObject<'gc>> {
        None
    }

    /// Unwrap this object as array storage.
    fn as_array_storage(&self) -> Option<Ref<ArrayStorage<'gc>>> {
        None
    }

    /// Unwrap this object as bytearray.
    fn as_bytearray(&self) -> Option<Ref<ByteArrayStorage>> {
        None
    }

    fn as_bytearray_mut(&self, _mc: MutationContext<'gc, '_>) -> Option<RefMut<ByteArrayStorage>> {
        None
    }

    fn as_bytearray_object(&self) -> Option<ByteArrayObject<'gc>> {
        None
    }

    /// Unwrap this object as mutable array storage.
    fn as_array_storage_mut(
        &self,
        _mc: MutationContext<'gc, '_>,
    ) -> Option<RefMut<ArrayStorage<'gc>>> {
        None
    }

    /// Unwrap this object as vector storage.
    fn as_vector_storage(&self) -> Option<Ref<VectorStorage<'gc>>> {
        None
    }

    /// Unwrap this object as mutable vector storage.
    fn as_vector_storage_mut(
        &self,
        _mc: MutationContext<'gc, '_>,
    ) -> Option<RefMut<VectorStorage<'gc>>> {
        None
    }

    /// Get this object's `DisplayObject`, if it has one.
    fn as_display_object(&self) -> Option<DisplayObject<'gc>> {
        None
    }

    /// Associate this object with a display object, if it can support such an
    /// association.
    ///
    /// If not, then this function does nothing.
    fn init_display_object(&self, _mc: MutationContext<'gc, '_>, _obj: DisplayObject<'gc>) {}

    /// Unwrap this object as an ApplicationDomain.
    fn as_application_domain(&self) -> Option<Domain<'gc>> {
        None
    }

    /// Unwrap this object as an event.
    fn as_event(&self) -> Option<Ref<Event<'gc>>> {
        None
    }

    /// Unwrap this object as a mutable event.
    fn as_event_mut(&self, _mc: MutationContext<'gc, '_>) -> Option<RefMut<Event<'gc>>> {
        None
    }

    /// Unwrap this object as a list of event handlers.
    fn as_dispatch(&self) -> Option<Ref<DispatchList<'gc>>> {
        None
    }

    /// Unwrap this object as a mutable list of event handlers.
    fn as_dispatch_mut(&self, _mc: MutationContext<'gc, '_>) -> Option<RefMut<DispatchList<'gc>>> {
        None
    }

    /// Unwrap this object as an immutable primitive value.
    ///
    /// This function should not be called in cases where a normal `Value`
    /// coercion would do. It *only* accounts for boxed primitives, and not
    /// `valueOf`.
    fn as_primitive(&self) -> Option<Ref<Value<'gc>>> {
        None
    }

    /// Unwrap this object as a mutable primitive value.
    fn as_primitive_mut(&self, _mc: MutationContext<'gc, '_>) -> Option<RefMut<Value<'gc>>> {
        None
    }

    /// Unwrap this object as a regexp.
    fn as_regexp(&self) -> Option<Ref<RegExp<'gc>>> {
        None
    }

    /// Unwrap this object as a mutable regexp.
    fn as_regexp_mut(&self, _mc: MutationContext<'gc, '_>) -> Option<RefMut<RegExp<'gc>>> {
        None
    }

    /// Unwrap this object's loader stream
    fn as_loader_stream(&self) -> Option<Ref<LoaderStream<'gc>>> {
        None
    }

    /// Unwrap this object's sound handle.
    fn as_sound(self) -> Option<SoundHandle> {
        None
    }

    /// Associate the object with a particular sound handle.
    ///
    /// This does nothing if the object is not a sound.
    fn set_sound(self, _mc: MutationContext<'gc, '_>, _sound: SoundHandle) {}

    /// Unwrap this object's sound instance handle.
    fn as_sound_channel(self) -> Option<SoundChannelObject<'gc>> {
        None
    }

    /// Associate the object with a particular sound instance handle.
    ///
    /// This does nothing if the object is not a sound channel.
    fn set_sound_instance(self, _mc: MutationContext<'gc, '_>, _sound: SoundInstanceHandle) {}

    /// Unwrap this object's bitmap data
    fn as_bitmap_data(&self) -> Option<GcCell<'gc, BitmapData<'gc>>> {
        None
    }

    /// Initialize the bitmap data in this object, if it's capable of
    /// supporting said data.
    ///
    /// This should only be called to initialize the association between an AVM
    /// object and it's associated bitmap data. This association should not be
    /// reinitialized later.
    fn init_bitmap_data(
        &self,
        _mc: MutationContext<'gc, '_>,
        _new_bitmap: GcCell<'gc, BitmapData<'gc>>,
    ) {
    }

    /// Get this objects `DateObject`, if it has one.
    fn as_date_object(&self) -> Option<DateObject<'gc>> {
        None
    }

    /// Get this object as a `DictionaryObject`, if it is one.
    fn as_dictionary_object(self) -> Option<DictionaryObject<'gc>> {
        None
    }

    /// Unwrap this object as a text format.
    fn as_text_format(&self) -> Option<Ref<TextFormat>> {
        None
    }

    /// Unwrap this object as a mutable text format.
    fn as_text_format_mut(&self, _mc: MutationContext<'gc, '_>) -> Option<RefMut<TextFormat>> {
        None
    }
}

pub enum ObjectPtr {}

impl<'gc> Object<'gc> {
    pub fn ptr_eq<T: TObject<'gc>>(a: T, b: T) -> bool {
        a.as_ptr() == b.as_ptr()
    }
}

impl<'gc> PartialEq for Object<'gc> {
    fn eq(&self, other: &Self) -> bool {
        Object::ptr_eq(*self, *other)
    }
}

impl<'gc> Eq for Object<'gc> {}

impl<'gc> Hash for Object<'gc> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ptr().hash(state);
    }
}
