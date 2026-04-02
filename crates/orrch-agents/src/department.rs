use serde::{Deserialize, Serialize};

/// Organizational departments for agent classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Department {
    Admin,
    Development,
    Marketing,
    Legal,
}

impl Department {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Admin => "Admin",
            Self::Development => "Development",
            Self::Marketing => "Marketing",
            Self::Legal => "Legal",
        }
    }
}

/// Predefined agent roles within the department hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    // Admin
    ExecutiveAssistant,
    ChiefOperationsOfficer,
    IntelligenceResourcesManager,
    Mentor,
    Hypervisor,

    // Development > Leadership
    ProjectManager,
    TalentScout,
    ResourceOptimizer,

    // Development > Engineering
    SoftwareEngineer,
    Developer,
    FeatureTester,
    Researcher,
    UiDesigner,
    Specialist,

    // Development > Quality Assurance
    PenetrationTester,
    BetaTester,

    // Development > DevOps
    RepositoryManager,

    // Marketing
    UxSpecialist,
    MarketResearcher,

    // Legal
    LicensingAuditor,
    CopyrightInvestigator,
}

impl AgentRole {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ExecutiveAssistant => "Executive Assistant",
            Self::ChiefOperationsOfficer => "Chief Operations Officer",
            Self::IntelligenceResourcesManager => "Intelligence Resources Manager",
            Self::Mentor => "Mentor",
            Self::Hypervisor => "Hypervisor",
            Self::ProjectManager => "Project Manager",
            Self::TalentScout => "Talent Scout",
            Self::ResourceOptimizer => "Resource Optimizer",
            Self::SoftwareEngineer => "Software Engineer",
            Self::Developer => "Developer",
            Self::FeatureTester => "Feature Tester",
            Self::Researcher => "Researcher",
            Self::UiDesigner => "UI Designer",
            Self::Specialist => "Specialist",
            Self::PenetrationTester => "Penetration Tester",
            Self::BetaTester => "Beta Tester",
            Self::RepositoryManager => "Repository Manager",
            Self::UxSpecialist => "UX Specialist",
            Self::MarketResearcher => "Market Researcher",
            Self::LicensingAuditor => "Licensing Auditor",
            Self::CopyrightInvestigator => "Copyright Investigator",
        }
    }

    pub fn department(&self) -> Department {
        match self {
            Self::ExecutiveAssistant
            | Self::ChiefOperationsOfficer
            | Self::IntelligenceResourcesManager
            | Self::Mentor
            | Self::Hypervisor => Department::Admin,

            Self::ProjectManager
            | Self::TalentScout
            | Self::ResourceOptimizer
            | Self::SoftwareEngineer
            | Self::Developer
            | Self::FeatureTester
            | Self::Researcher
            | Self::UiDesigner
            | Self::Specialist
            | Self::PenetrationTester
            | Self::BetaTester
            | Self::RepositoryManager => Department::Development,

            Self::UxSpecialist | Self::MarketResearcher => Department::Marketing,

            Self::LicensingAuditor | Self::CopyrightInvestigator => Department::Legal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_roles_have_departments() {
        let roles = [
            AgentRole::ExecutiveAssistant, AgentRole::ChiefOperationsOfficer,
            AgentRole::IntelligenceResourcesManager, AgentRole::Mentor, AgentRole::Hypervisor,
            AgentRole::ProjectManager, AgentRole::TalentScout, AgentRole::ResourceOptimizer,
            AgentRole::SoftwareEngineer, AgentRole::Developer, AgentRole::FeatureTester,
            AgentRole::Researcher, AgentRole::UiDesigner, AgentRole::Specialist,
            AgentRole::PenetrationTester, AgentRole::BetaTester, AgentRole::RepositoryManager,
            AgentRole::UxSpecialist, AgentRole::MarketResearcher,
            AgentRole::LicensingAuditor, AgentRole::CopyrightInvestigator,
        ];
        assert_eq!(roles.len(), 21); // 19 from plan + Hypervisor + Resource Optimizer
        for role in &roles {
            let _ = role.department();
            let _ = role.label();
        }
    }

    #[test]
    fn test_department_labels() {
        assert_eq!(Department::Admin.label(), "Admin");
        assert_eq!(Department::Development.label(), "Development");
    }
}
