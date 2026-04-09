pub mod profile;
pub mod department;
pub mod runner;

pub use profile::{AgentProfile, load_agents, agents_dir};
pub use department::{Department, AgentRole};
pub use runner::{
    AgentRunner,
    ResourceKind,
    ResourceUpdateRequest,
    build_hypervisor_context,
    build_researcher_resource_prompt,
    is_verification_role,
    load_project_core_context,
    mentor_review_profile,
};
