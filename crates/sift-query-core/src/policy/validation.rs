use super::{QueryPolicy, is_name};
use crate::{Codec, ParamError};
use std::any::TypeId;
use std::sync::Arc;

impl QueryPolicy {
    /// Iterates the codecs that are actually bound to an entity's result
    /// fields. The binding list is validated and cached per policy/entity.
    pub fn for_each_codec_binding<E: crate::SearchEntity>(
        &self,
        mut callback: impl FnMut(&'static crate::FieldMeta, &dyn Codec),
    ) -> Result<(), ParamError> {
        let key = TypeId::of::<E>();
        let bindings = {
            let mut cache = self
                .codec_bindings
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache.get(&key).cloned()
        };
        let bindings = if let Some(bindings) = bindings {
            bindings?
        } else {
            let result = self.build_codec_bindings::<E>();
            self.codec_bindings
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(key, result.clone());
            result?
        };
        for binding in bindings.iter() {
            callback(binding.field, binding.codec.as_ref());
        }
        Ok(())
    }

    fn build_codec_bindings<E: crate::SearchEntity>(
        &self,
    ) -> Result<Arc<[super::CodecBinding]>, ParamError> {
        self.validate_codecs::<E>()?;
        let bindings = E::META
            .fields
            .iter()
            .filter_map(|field| {
                self.codec_arc_for::<E>(field)
                    .map(|codec| super::CodecBinding { field, codec })
            })
            .collect::<Vec<_>>();
        Ok(Arc::from(bindings))
    }

    pub(crate) fn validate_operators<E: crate::SearchEntity>(&self) -> Result<(), ParamError> {
        let key = TypeId::of::<E>();
        {
            let mut cache = self
                .validated_operators
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(result) = cache.get(&key) {
                return result.clone();
            }
        }
        let result = self.validate_operators_uncached::<E>();
        self.validated_operators
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, result.clone());
        result
    }

    fn validate_operators_uncached<E: crate::SearchEntity>(&self) -> Result<(), ParamError> {
        for field in E::META.fields {
            let mut names = std::collections::HashSet::new();
            for name in E::custom_operator_names(field.id) {
                let registration =
                    self.registered_operator(name)
                        .ok_or_else(|| ParamError::InvalidValue {
                            field: field.public_name.to_owned(),
                            reason: "custom operator is not registered".to_owned(),
                        })?;
                if !names.insert(*name)
                    || registration.field_type != field.value_type
                    || !field.filterable
                {
                    return Err(ParamError::InvalidValue {
                        field: field.public_name.to_owned(),
                        reason: "duplicate or incompatible custom operator whitelist".to_owned(),
                    });
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_entity_names<E: crate::SearchEntity>(&self) -> Result<(), ParamError> {
        let key = TypeId::of::<E>();
        {
            let mut cache = self
                .validated_names
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(result) = cache.get(&key) {
                return result.clone();
            }
        }
        let result = crate::protocol::parser::validate_entity_names::<E>(self);
        self.validated_names
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, result.clone());
        result
    }

    pub fn codec_for<E: crate::SearchEntity>(
        &self,
        field: &crate::FieldMeta,
    ) -> Option<&dyn Codec> {
        match E::codec_name(field.id) {
            Some(name) => self
                .codecs
                .iter()
                .find(|codec| codec.name() == name)
                .map(Arc::as_ref),
            None => self.codecs.find(field),
        }
    }

    fn codec_arc_for<E: crate::SearchEntity>(
        &self,
        field: &crate::FieldMeta,
    ) -> Option<Arc<dyn Codec>> {
        match E::codec_name(field.id) {
            Some(name) => self
                .codecs
                .iter()
                .find(|codec| codec.name() == name)
                .cloned(),
            None => self.codecs.find_arc(field),
        }
    }

    pub(crate) fn validate_codecs<E: crate::SearchEntity>(&self) -> Result<(), ParamError> {
        let key = TypeId::of::<E>();
        {
            let mut cache = self
                .validated_codecs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(result) = cache.get(&key) {
                return result.clone();
            }
        }
        let result = self.validate_codecs_uncached::<E>();
        self.validated_codecs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, result.clone());
        result
    }

    fn validate_codecs_uncached<E: crate::SearchEntity>(&self) -> Result<(), ParamError> {
        let mut names = std::collections::HashSet::new();
        for codec in self.codecs.iter() {
            if !is_name(codec.name()) || !names.insert(codec.name()) {
                return Err(ParamError::ConflictingParameters(
                    "invalid or duplicate codec registration".to_owned(),
                ));
            }
        }
        for field in E::META.fields {
            if E::codec_name(field.id).is_some() {
                if self
                    .codec_for::<E>(field)
                    .is_none_or(|codec| !codec.supports(field))
                {
                    return Err(ParamError::InvalidValue {
                        field: field.public_name.to_owned(),
                        reason: "named codec is missing or incompatible".to_owned(),
                    });
                }
            } else if self
                .codecs
                .iter()
                .filter(|codec| codec.supports(field))
                .count()
                > 1
            {
                return Err(ParamError::ConflictingParameters(
                    "ambiguous codec binding".to_owned(),
                ));
            }
        }
        Ok(())
    }
}
