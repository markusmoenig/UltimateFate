macro_rules! define_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(pub u64);

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

define_id!(PersonId);
define_id!(FamilyId);
define_id!(FactionId);
define_id!(SiteId);
define_id!(EventId);
define_id!(LawId);
define_id!(ClaimId);
define_id!(ProjectId);
define_id!(WorldItemId);
define_id!(RouteId);
define_id!(GoalId);
define_id!(PartyId);
