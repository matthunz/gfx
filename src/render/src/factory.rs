// Copyright 2014 The Gfx-rs Developers.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Factory extension.
//!
//! This module serves as an extension to the `factory` module in the `gfx` crate. This module
//! exposes extension functions and shortcuts to aid with creating and managing graphics resources.
//! See the `FactoryExt` trait for more information.

use gfx_core::{format, handle, tex, state};
use gfx_core::{Primitive, Resources, ShaderSet};
use gfx_core::factory::{Bind, BufferRole, Factory};
use gfx_core::pso::{CreationError, Descriptor};
use slice::{Slice, IndexBuffer, IntoIndexBuffer};
use pso;
use shade::ProgramError;

/// Error creating a PipelineState
#[derive(Clone, PartialEq, Debug)]
pub enum PipelineStateError {
    /// Shader program failed to link.
    Program(ProgramError),
    /// Unable to create PSO descriptor due to mismatched formats.
    DescriptorInit(pso::InitError),
    /// Device failed to create the handle give the descriptor.
    DeviceCreate(CreationError),
}


/// This trait is responsible for creating and managing graphics resources, much like the `Factory`
/// trait in the `gfx` crate. Every `Factory` automatically implements `FactoryExt`. 
pub trait FactoryExt<R: Resources>: Factory<R> {
    /// Create a vertex buffer from the supplied data. A `Slice` will have to manually be
    /// constructed.
    fn create_vertex_buffer<T>(&mut self, vertices: &[T])
                            -> handle::Buffer<R, T> where
                            T: Copy + pso::buffer::Structure<format::Format>
    {
        //debug_assert!(nv <= self.get_capabilities().max_vertex_count);
        self.create_buffer_const(vertices, BufferRole::Vertex, Bind::empty()).unwrap()
    }
    
    /// Shorthand for creating a new vertex buffer from the supplied vertices, together with a
    /// `Slice` from the supplied indices.
    fn create_vertex_buffer_with_slice<B, V>(&mut self, vertices: &[V], indices: B)
                                       -> (handle::Buffer<R, V>, Slice<R>)
                                       where V: Copy + pso::buffer::Structure<format::Format>,
                                             B: IntoIndexBuffer<R>
    {
        let vertex_buffer = self.create_vertex_buffer(vertices);
        let index_buffer = indices.into_index_buffer(self);
        let buffer_length = match index_buffer {
            IndexBuffer::Auto => vertex_buffer.len(),
            IndexBuffer::Index16(ref ib) => ib.len(),
            IndexBuffer::Index32(ref ib) => ib.len(),
        };
        
        (vertex_buffer, Slice {
            start: 0,
            end: buffer_length as u32,
            base_vertex: 0,
            instances: None,
            buffer: index_buffer
        })
    }

    /// Create a constant buffer for `num` identical elements of type `T`.
    fn create_constant_buffer<T>(&mut self, num: usize) -> handle::Buffer<R, T> {
        self.create_buffer_dynamic(num, BufferRole::Uniform, Bind::empty())
            .unwrap()
    }

    /// Creates a `ShaderSet` from the supplied vertex and pixel shader source code.
    fn create_shader_set(&mut self, vs_code: &[u8], ps_code: &[u8])
                         -> Result<ShaderSet<R>, ProgramError> {
        let vs = match self.create_shader_vertex(vs_code) {
            Ok(s) => s,
            Err(e) => return Err(ProgramError::Vertex(e)),
        };
        let ps = match self.create_shader_pixel(ps_code) {
            Ok(s) => s,
            Err(e) => return Err(ProgramError::Pixel(e)),
        };
        Ok(ShaderSet::Simple(vs, ps))
    }

    /// Creates a basic shader `Program` from the supplied vertex and pixel shader source code.
    fn link_program(&mut self, vs_code: &[u8], ps_code: &[u8])
                    -> Result<handle::Program<R>, ProgramError> {

        let set = try!(self.create_shader_set(vs_code, ps_code));
        self.create_program(&set)
            .map_err(|e| ProgramError::Link(e))
    }

    /// Similar to `create_pipeline_from_program(..)`, but takes a `ShaderSet` as opposed to a
    /// shader `Program`.  
    fn create_pipeline_state<I: pso::PipelineInit>(&mut self, shaders: &ShaderSet<R>,
                             primitive: Primitive, rasterizer: state::Rasterizer, init: I)
                             -> Result<pso::PipelineState<R, I::Meta>, PipelineStateError>
    {
        match self.create_program(shaders) {
            Ok(p) => self.create_pipeline_from_program(&p, primitive, rasterizer, init),
            Err(e) => Err(PipelineStateError::Program(ProgramError::Link(e))),
        }
    }

    /// Creates a strongly typed `PipelineState` from its `Init` structure, a shader `Program`, a
    /// primitive type and a `Rasterizer`.
    fn create_pipeline_from_program<I: pso::PipelineInit>(&mut self, program: &handle::Program<R>,
                                    primitive: Primitive, rasterizer: state::Rasterizer, init: I)
                                    -> Result<pso::PipelineState<R, I::Meta>, PipelineStateError>
    {
        let mut descriptor = Descriptor::new(primitive, rasterizer);
        let meta = match init.link_to(&mut descriptor, program.get_info()) {
            Ok(m) => m,
            Err(e) => return Err(PipelineStateError::DescriptorInit(e)),
        };
        let raw = match self.create_pipeline_state_raw(program, &descriptor) {
            Ok(raw) => raw,
            Err(e) => return Err(PipelineStateError::DeviceCreate(e)),
        };

        Ok(pso::PipelineState::new(raw, primitive, meta))
    }

    /// Creates a strongly typed `PipelineState` from its `Init` structure. Automatically creates a
    /// shader `Program` from a vertex and pixel shader source, as well as a `Rasterizer` capable
    /// of rendering triangle faces without culling.
    fn create_pipeline_simple<I: pso::PipelineInit>(&mut self, vs: &[u8], ps: &[u8], init: I)
                              -> Result<pso::PipelineState<R, I::Meta>, PipelineStateError>
    {
        match self.create_shader_set(vs, ps) {
            Ok(ref s) => self.create_pipeline_state(s,
                Primitive::TriangleList, state::Rasterizer::new_fill(), init),
            Err(e) => Err(PipelineStateError::Program(e)),
        }
    }

    /// Create a linear sampler with clamping to border.
    fn create_sampler_linear(&mut self) -> handle::Sampler<R> {
        self.create_sampler(tex::SamplerInfo::new(
            tex::FilterMethod::Trilinear,
            tex::WrapMode::Clamp,
        ))
    }
}

impl<R: Resources, F: Factory<R>> FactoryExt<R> for F {}
