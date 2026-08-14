pub mod department;
pub mod profile;
pub mod runner;

pub use department::{AgentRole, Department};
pub use profile::{AgentProfile, Importance, agent_layer_engine, agents_dir, load_agents};
pub use runner::{
    AgentRunner, ResourceKind, ResourceUpdateRequest, build_hypervisor_context,
    build_researcher_resource_prompt, is_verification_role, load_project_core_context,
    mentor_review_profile,
};
