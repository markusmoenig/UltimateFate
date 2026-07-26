//! Player-facing narrative assembled from structured simulation state.
//!
//! This layer may phrase facts and beliefs, but it never changes authoritative
//! history and never exposes a claim's objective truth value to the player.

use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{Display, Formatter, Write},
};

use ultimate_fate_history::{
    Belief, BeliefSource, ClaimAudience, ClaimId, ClaimOrigin, EntityRef, EventId, EventPublicity,
    FactionId, HistoricalEvent, HistoricalEventKind, HistoricalWorld, LawId, LawKind, Occupation,
    PersonId, PhysicalEvidence, PhysicalEvidenceKind, Proposition, ResourceKind, SiteId, WorldDate,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerBackground {
    Outsider,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BriefingSectionKind {
    Setting,
    PresentCrisis,
    PublicDispute,
    Arrival,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcedParagraph {
    pub kind: BriefingSectionKind,
    pub text: String,
    pub events: Vec<EventId>,
    pub claims: Vec<ClaimId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryHookTarget {
    TalkTo(PersonId),
    ExamineEvidence(EventId),
    AskAboutClaim(ClaimId),
    ReviewLaw(LawId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoryHook {
    pub title: String,
    pub description: String,
    pub target: StoryHookTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignBriefing {
    pub title: String,
    pub date: WorldDate,
    pub location: SiteId,
    pub background: PlayerBackground,
    pub paragraphs: Vec<SourcedParagraph>,
    pub known_factions: Vec<FactionId>,
    pub known_laws: Vec<LawId>,
    pub rendered_text: String,
}

impl CampaignBriefing {
    pub fn referenced_events(&self) -> BTreeSet<EventId> {
        self.paragraphs
            .iter()
            .flat_map(|paragraph| paragraph.events.iter().copied())
            .collect()
    }

    pub fn referenced_claims(&self) -> BTreeSet<ClaimId> {
        self.paragraphs
            .iter()
            .flat_map(|paragraph| paragraph.claims.iter().copied())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalSource {
    ArrivalBriefing,
    Conversation(PersonId),
    PhysicalEvidence(EventId),
    Combat(EventId),
    Resolution(EventId),
    Quest(EventId),
    WorldChange(EventId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalEntry {
    pub title: String,
    pub body: String,
    pub learned_at: WorldDate,
    pub source: JournalSource,
    pub events: Vec<EventId>,
    pub claims: Vec<ClaimId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlayerKnowledge {
    pub known_events: BTreeSet<EventId>,
    pub known_claims: BTreeSet<ClaimId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Journal {
    pub entries: Vec<JournalEntry>,
    pub knowledge: PlayerKnowledge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignStart {
    pub briefing: CampaignBriefing,
    pub journal: Journal,
    pub hooks: Vec<StoryHook>,
    pub arrival_contact: PersonId,
    pub lead_evidence: Option<EventId>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConversationTopicKind {
    Orientation,
    PresentCrisis,
    FactionView,
    Claim(ClaimId),
    Law(LawId),
    Evidence(EventId),
    Aftermath(EventId),
    RequestAid(PersonId),
    OfferPayment(PersonId),
    OfferAid(PersonId),
    SupportAid(PersonId),
    TakeAid(PersonId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationTopic {
    pub kind: ConversationTopicKind,
    pub prompt: String,
    pub response: String,
    pub reveals_events: Vec<EventId>,
    pub reveals_claims: Vec<ClaimId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conversation {
    pub speaker: PersonId,
    pub speaker_name: String,
    pub occupation: Occupation,
    pub faction: FactionId,
    pub faction_name: String,
    pub topics: Vec<ConversationTopic>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConversationContext {
    pub examined_evidence: BTreeSet<EventId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationError {
    MissingSpeaker(PersonId),
}

impl Display for ConversationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ConversationError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignStartError {
    MissingSite(SiteId),
    MissingFoundation,
    MissingArrivalContact,
}

impl Display for CampaignStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for CampaignStartError {}

impl Conversation {
    pub fn for_person(
        world: &HistoricalWorld,
        start: &CampaignStart,
        speaker: PersonId,
        context: &ConversationContext,
    ) -> Result<Self, ConversationError> {
        let person = world
            .people()
            .get(&speaker)
            .ok_or(ConversationError::MissingSpeaker(speaker))?;
        let faction = &world.factions()[&person.faction];
        let mut topics = Vec::new();

        if speaker == start.arrival_contact {
            let response = match start.lead_evidence {
                Some(event) => {
                    format!(
                        "Begin with {}. Look at what remains, then ask people why they remember it differently.",
                        evidence_name_for_event(world, start.briefing.location, event)
                    )
                }
                None => {
                    "Begin with the public record, then ask who paid for the choices it leaves out."
                        .to_string()
                }
            };
            topics.push(ConversationTopic {
                kind: ConversationTopicKind::Orientation,
                prompt: "Where should I begin?".to_string(),
                response,
                reveals_events: Vec::new(),
                reveals_claims: Vec::new(),
            });
        }

        if let Some(crisis) = start
            .briefing
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.kind == BriefingSectionKind::PresentCrisis)
        {
            topics.push(ConversationTopic {
                kind: ConversationTopicKind::PresentCrisis,
                prompt: "What is happening here?".to_string(),
                response: crisis.text.clone(),
                reveals_events: crisis.events.clone(),
                reveals_claims: crisis.claims.clone(),
            });
        }

        topics.push(ConversationTopic {
            kind: ConversationTopicKind::FactionView,
            prompt: format!("What does {} stand for?", faction.name),
            response: format!(
                "{} says it acts from {}. Its rivals would describe the same choices less kindly.",
                faction.name,
                principle_name(faction.principle)
            ),
            reveals_events: Vec::new(),
            reveals_claims: Vec::new(),
        });

        if let Some(intervention) = world.events().values().rev().find(|event| {
            event.location == start.briefing.location
                && event.kind == HistoricalEventKind::PlayerIntervention
        }) {
            topics.push(ConversationTopic {
                kind: ConversationTopicKind::Aftermath(intervention.id),
                prompt: "What do you think of my intervention?".to_string(),
                response: faction_reaction(world, intervention, person.faction),
                reveals_events: vec![intervention.id],
                reveals_claims: Vec::new(),
            });
        }

        if let Some(beliefs) = world.knowledge().beliefs.get(&speaker) {
            let crisis_events: BTreeSet<_> = start
                .briefing
                .paragraphs
                .iter()
                .filter(|paragraph| paragraph.kind == BriefingSectionKind::PresentCrisis)
                .flat_map(|paragraph| paragraph.events.iter().copied())
                .collect();
            let mut shareable = beliefs
                .values()
                .filter(|belief| belief.willing_to_share)
                .collect::<Vec<_>>();
            shareable.sort_by(|first, second| {
                belief_priority(world, start, &crisis_events, second).cmp(&belief_priority(
                    world,
                    start,
                    &crisis_events,
                    first,
                ))
            });
            for belief in shareable.into_iter().take(2) {
                let Some(claim) = world.claims().get(&belief.claim) else {
                    continue;
                };
                topics.push(ConversationTopic {
                    kind: ConversationTopicKind::Claim(claim.id),
                    prompt: claim_prompt(world, &claim.proposition),
                    response: belief_response(world, belief, &claim.proposition),
                    reveals_events: proposition_events(&claim.proposition, claim.created_by_event),
                    reveals_claims: vec![claim.id],
                });
            }
        }

        if let Some(law) = world.sites()[&start.briefing.location]
            .laws
            .values()
            .find(|law| law.active)
        {
            let authority = &world.factions()[&law.authority];
            topics.push(ConversationTopic {
                kind: ConversationTopicKind::Law(law.id),
                prompt: format!("Why is {} in force?", law_name(law.kind)),
                response: format!(
                    "{} imposed {} and justified it through {}. Whether necessity excuses its cost is still disputed.",
                    authority.name,
                    law_name(law.kind),
                    principle_name(law.justification)
                ),
                reveals_events: public_law_event(world, law.id)
                    .map(|event| vec![event.id])
                    .unwrap_or_default(),
                reveals_claims: Vec::new(),
            });
        }

        if let Some(event) = start
            .lead_evidence
            .filter(|event| context.examined_evidence.contains(event))
        {
            let evidence_name = evidence_name_for_event(world, start.briefing.location, event);
            let related_claim = world.knowledge().beliefs.get(&speaker).and_then(|beliefs| {
                beliefs.values().find(|belief| {
                    belief.willing_to_share
                        && world.claims().get(&belief.claim).is_some_and(|claim| {
                            proposition_events(&claim.proposition, claim.created_by_event)
                                .contains(&event)
                        })
                })
            });
            let (response, claims) = related_claim
                .and_then(|belief| {
                    world.claims().get(&belief.claim).map(|claim| {
                        (
                            format!(
                                "{} As for {}, it supports that account, but it does not settle who benefited.",
                                belief_response(world, belief, &claim.proposition),
                                evidence_name
                            ),
                            vec![claim.id],
                        )
                    })
                })
                .unwrap_or_else(|| {
                    (
                        format!(
                            "{} is real, but people are making it carry more certainty than it can bear. {} judges it through {}.",
                            evidence_name,
                            faction.name,
                            principle_name(faction.principle)
                        ),
                        Vec::new(),
                    )
                });
            topics.push(ConversationTopic {
                kind: ConversationTopicKind::Evidence(event),
                prompt: format!("What do you make of {evidence_name}?"),
                response,
                reveals_events: vec![event],
                reveals_claims: claims,
            });
        }

        let mut seen = BTreeSet::new();
        topics.retain(|topic| seen.insert(topic.prompt.clone()));
        topics.truncate(7);

        Ok(Self {
            speaker,
            speaker_name: full_name(world, speaker),
            occupation: person.occupation,
            faction: person.faction,
            faction_name: faction.name.clone(),
            topics,
        })
    }
}

impl CampaignStart {
    pub fn for_outsider(
        world: &HistoricalWorld,
        location: SiteId,
    ) -> Result<Self, CampaignStartError> {
        let site = world
            .sites()
            .get(&location)
            .ok_or(CampaignStartError::MissingSite(location))?;
        let foundation = world
            .events()
            .values()
            .find(|event| {
                event.location == location
                    && event.kind == HistoricalEventKind::SettlementFounded
                    && event.publicity == EventPublicity::Public
            })
            .ok_or(CampaignStartError::MissingFoundation)?;
        let latest_shortage =
            latest_public_event(world, location, HistoricalEventKind::ShortageRecognized);
        let latest_protest = latest_public_event(world, location, HistoricalEventKind::Protest);
        let known_factions: Vec<_> = world.factions().keys().copied().collect();
        let known_laws: Vec<_> = site
            .laws
            .values()
            .filter(|law| law.active)
            .map(|law| law.id)
            .collect();
        let public_accusation = world.claims().values().rev().find(|claim| {
            claim.audience == ClaimAudience::Public
                && matches!(claim.proposition, Proposition::FactionResponsibleFor { .. })
        });
        let arrival_contact = choose_arrival_contact(world, location)
            .ok_or(CampaignStartError::MissingArrivalContact)?;
        let lead_evidence =
            select_lead_evidence(world, site, latest_shortage.map(|event| event.id));

        let mut paragraphs = Vec::new();
        paragraphs.push(SourcedParagraph {
            kind: BriefingSectionKind::Setting,
            text: setting_text(world, location, foundation),
            events: vec![foundation.id],
            claims: Vec::new(),
        });

        if let Some(shortage) = latest_shortage {
            let law_events: Vec<_> = known_laws
                .iter()
                .filter_map(|law| public_law_event(world, *law))
                .collect();
            let mut events = vec![shortage.id];
            events.extend(law_events.iter().map(|event| event.id));
            paragraphs.push(SourcedParagraph {
                kind: BriefingSectionKind::PresentCrisis,
                text: crisis_text(world, location, shortage, &known_laws),
                events,
                claims: Vec::new(),
            });
        }

        if public_accusation.is_some() || latest_protest.is_some() {
            let mut text = String::new();
            let mut events = Vec::new();
            let mut claims = Vec::new();
            if let Some(claim) = public_accusation {
                text.push_str(&public_claim_text(world, claim.id));
                events.push(claim.created_by_event);
                claims.push(claim.id);
            }
            if let Some(protest) = latest_protest {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&protest_text(world, protest));
                events.push(protest.id);
            }
            paragraphs.push(SourcedParagraph {
                kind: BriefingSectionKind::PublicDispute,
                text,
                events,
                claims,
            });
        }

        paragraphs.push(SourcedParagraph {
            kind: BriefingSectionKind::Arrival,
            text: arrival_text(world, arrival_contact, site.name.as_str(), lead_evidence),
            events: Vec::new(),
            claims: Vec::new(),
        });

        let title = format!("Year {} — {}", world.date.year, site.name);
        let rendered_text = render_briefing(&title, &paragraphs);
        let briefing = CampaignBriefing {
            title,
            date: world.date,
            location,
            background: PlayerBackground::Outsider,
            paragraphs,
            known_factions,
            known_laws: known_laws.clone(),
            rendered_text,
        };
        let known_events = briefing.referenced_events();
        let known_claims = briefing.referenced_claims();
        let hooks = build_hooks(
            world,
            arrival_contact,
            lead_evidence,
            public_accusation.map(|claim| claim.id),
            &known_laws,
        );
        let journal = Journal {
            entries: vec![JournalEntry {
                title: format!("Arrival in {}", site.name),
                body: journal_text(world, location, arrival_contact, &known_laws),
                learned_at: world.date,
                source: JournalSource::ArrivalBriefing,
                events: known_events.iter().copied().collect(),
                claims: known_claims.iter().copied().collect(),
            }],
            knowledge: PlayerKnowledge {
                known_events,
                known_claims,
            },
        };

        Ok(Self {
            briefing,
            journal,
            hooks,
            arrival_contact,
            lead_evidence: lead_evidence.map(|evidence| evidence.originating_event),
        })
    }
}

fn latest_public_event(
    world: &HistoricalWorld,
    location: SiteId,
    kind: HistoricalEventKind,
) -> Option<&HistoricalEvent> {
    world.events().values().rev().find(|event| {
        event.location == location
            && event.kind == kind
            && event.publicity == EventPublicity::Public
    })
}

fn public_law_event(world: &HistoricalWorld, law: LawId) -> Option<&HistoricalEvent> {
    world.events().values().rev().find(|event| {
        event.publicity == EventPublicity::Public
            && event.participants.contains(&EntityRef::Law(law))
            && event.kind == HistoricalEventKind::LawEnacted
    })
}

fn select_lead_evidence<'a>(
    world: &HistoricalWorld,
    site: &'a ultimate_fate_history::Site,
    crisis: Option<EventId>,
) -> Option<&'a PhysicalEvidence> {
    site.physical_evidence
        .iter()
        .rev()
        .find(|evidence| {
            crisis.is_some_and(|crisis| {
                evidence.originating_event == crisis
                    || world
                        .causal_ancestors(evidence.originating_event)
                        .contains(&crisis)
            })
        })
        .or_else(|| site.physical_evidence.first())
}

fn choose_arrival_contact(world: &HistoricalWorld, location: SiteId) -> Option<PersonId> {
    let priorities = [
        Occupation::Innkeeper,
        Occupation::Merchant,
        Occupation::Healer,
        Occupation::Priest,
    ];
    for occupation in priorities {
        if let Some(person) = world
            .living_people()
            .find(|person| person.home == location && person.occupation == occupation)
        {
            return Some(person.id);
        }
    }
    world
        .living_people()
        .find(|person| person.home == location)
        .map(|person| person.id)
}

fn setting_text(world: &HistoricalWorld, location: SiteId, foundation: &HistoricalEvent) -> String {
    let site = &world.sites()[&location];
    let population = world
        .living_people()
        .filter(|person| person.home == location)
        .count();
    let faction_text = world
        .factions()
        .values()
        .map(|faction| {
            format!(
                "{} under {}, guided publicly by {}",
                faction.name,
                full_name(world, faction.leader),
                principle_name(faction.principle)
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "{} is now home to roughly {population} people. According to public histories, {}. Three factions shape daily life: {faction_text}.",
        site.name, foundation.summary
    )
}

fn crisis_text(
    world: &HistoricalWorld,
    location: SiteId,
    shortage: &HistoricalEvent,
    laws: &[LawId],
) -> String {
    let site = &world.sites()[&location];
    let population = world
        .living_people()
        .filter(|person| person.home == location)
        .count() as i64;
    let food = site
        .resources
        .get(&ResourceKind::Food)
        .copied()
        .unwrap_or_default();
    let condition = if food < population {
        "critically low"
    } else if food < population * 3 {
        "dangerously strained"
    } else {
        "still recovering"
    };
    let law_text = if laws.is_empty() {
        "No emergency law is currently recorded.".to_string()
    } else {
        let descriptions = laws
            .iter()
            .map(|law| {
                let law = &site.laws[law];
                format!(
                    "{} justified through {}",
                    law_name(law.kind),
                    principle_name(law.justification)
                )
            })
            .collect::<Vec<_>>()
            .join(" and ");
        format!("The town currently lives under {descriptions}.")
    };
    format!(
        "The food reserve is {condition}. The latest public notice reports: {}. {law_text}",
        shortage.summary
    )
}

fn protest_text(world: &HistoricalWorld, protest: &HistoricalEvent) -> String {
    let faction = protest
        .participants
        .iter()
        .find_map(|participant| match participant {
            EntityRef::Faction(faction) => Some(*faction),
            _ => None,
        });
    let law = protest
        .participants
        .iter()
        .find_map(|participant| match participant {
            EntityRef::Law(law) => Some(*law),
            _ => None,
        });
    match (faction, law) {
        (Some(faction), Some(law)) => {
            let law = world.sites().values().find_map(|site| site.laws.get(&law));
            match law {
                Some(law) => format!(
                    "{} has publicly protested {}.",
                    world.factions()[&faction].name,
                    law_name(law.kind)
                ),
                None => format!(
                    "{} has publicly protested the emergency law.",
                    world.factions()[&faction].name
                ),
            }
        }
        _ => "Public records describe protests against the emergency response.".to_string(),
    }
}

fn public_claim_text(world: &HistoricalWorld, claim: ClaimId) -> String {
    let claim = &world.claims()[&claim];
    match (&claim.origin, &claim.proposition) {
        (
            ClaimOrigin::Faction(origin),
            Proposition::FactionResponsibleFor {
                event,
                faction: blamed,
            },
        ) => format!(
            "{} publicly blames {} for the shortage declared in year {}.",
            world.factions()[origin].name,
            world.factions()[blamed].name,
            world.events()[event].date.year
        ),
        (_, proposition) => format!(
            "A public claim holds that {}.",
            world.describe_claim(proposition)
        ),
    }
}

fn arrival_text(
    world: &HistoricalWorld,
    contact: PersonId,
    town: &str,
    evidence: Option<&PhysicalEvidence>,
) -> String {
    let person = &world.people()[&contact];
    let lead = evidence
        .map(|evidence| physical_evidence_name(evidence.kind))
        .unwrap_or("the source of the town's present dispute");
    format!(
        "You arrive in {town} as an outsider. {}, a {}, has offered you a place to begin and suggests learning why {lead} has become the center of so many arguments.",
        full_name(world, contact),
        occupation_name(person.occupation)
    )
}

fn build_hooks(
    world: &HistoricalWorld,
    contact: PersonId,
    lead_evidence: Option<&PhysicalEvidence>,
    accusation: Option<ClaimId>,
    laws: &[LawId],
) -> Vec<StoryHook> {
    let mut hooks = vec![StoryHook {
        title: format!("Speak with {}", full_name(world, contact)),
        description: "Ask what has changed in town and which stories can be trusted.".to_string(),
        target: StoryHookTarget::TalkTo(contact),
    }];
    if let Some(evidence) = lead_evidence {
        hooks.push(StoryHook {
            title: format!("Visit {}", physical_evidence_name(evidence.kind)),
            description: "Examine where the town's recorded history and present crisis meet."
                .to_string(),
            target: StoryHookTarget::ExamineEvidence(evidence.originating_event),
        });
    }
    if let Some(claim) = accusation {
        hooks.push(StoryHook {
            title: "Ask who caused the shortage".to_string(),
            description: "Compare the public accusation with what different factions remember."
                .to_string(),
            target: StoryHookTarget::AskAboutClaim(claim),
        });
    }
    if let Some(law) = laws.first() {
        hooks.push(StoryHook {
            title: "Read the emergency law".to_string(),
            description: "Learn what the law demands and how its authors justify it.".to_string(),
            target: StoryHookTarget::ReviewLaw(*law),
        });
    }
    hooks
}

pub fn physical_evidence_name(kind: PhysicalEvidenceKind) -> &'static str {
    match kind {
        PhysicalEvidenceKind::PublicGranary => "the old common granary",
        PhysicalEvidenceKind::Fortification => "the old fortifications",
        PhysicalEvidenceKind::RefugeeDistrict => "the refugee district",
        PhysicalEvidenceKind::AbandonedFarm => "the abandoned farm",
        PhysicalEvidenceKind::Grave => "the disputed grave",
        PhysicalEvidenceKind::Memorial => "the public memorial",
        PhysicalEvidenceKind::BurnedBuilding => "the burned building",
    }
}

fn evidence_name_for_event(world: &HistoricalWorld, location: SiteId, event: EventId) -> String {
    world.sites()[&location]
        .physical_evidence
        .iter()
        .find(|evidence| evidence.originating_event == event)
        .map(|evidence| physical_evidence_name(evidence.kind).to_string())
        .unwrap_or_else(|| "the surviving evidence".to_string())
}

fn claim_prompt(world: &HistoricalWorld, proposition: &Proposition) -> String {
    match proposition {
        Proposition::EventOccurred(event) => event_question(world, *event),
        Proposition::FactionResponsibleFor { event, .. } => responsibility_question(world, *event),
        Proposition::LawWasNecessary(law) => world
            .sites()
            .values()
            .find_map(|site| site.laws.get(law))
            .map(|law| format!("Was {} necessary?", law_name(law.kind)))
            .unwrap_or_else(|| "Was the emergency law necessary?".to_string()),
        Proposition::PersonDied(person) => {
            format!("What happened to {}?", full_name(world, *person))
        }
        Proposition::ObjectSurvived(_) => "Did the lost object survive the crisis?".to_string(),
        Proposition::FormulaProduces { formula, .. } => {
            format!("What does {} accomplish?", formula_name(world, *formula))
        }
        Proposition::FormulaRequires { formula, .. } => {
            format!("What does {} require?", formula_name(world, *formula))
        }
    }
}

fn belief_priority(
    world: &HistoricalWorld,
    start: &CampaignStart,
    crisis_events: &BTreeSet<EventId>,
    belief: &Belief,
) -> (bool, WorldDate, ClaimId) {
    let claim = &world.claims()[&belief.claim];
    let events = proposition_events(&claim.proposition, claim.created_by_event);
    let relevant = events
        .iter()
        .any(|event| Some(*event) == start.lead_evidence || crisis_events.contains(event));
    (
        relevant,
        world.events()[&claim.created_by_event].date,
        claim.id,
    )
}

fn faction_reaction(
    world: &HistoricalWorld,
    intervention: &HistoricalEvent,
    faction: FactionId,
) -> String {
    let participating_factions = intervention
        .participants
        .iter()
        .filter_map(|participant| match participant {
            EntityRef::Faction(faction) => Some(*faction),
            _ => None,
        })
        .collect::<Vec<_>>();
    let authority = participating_factions.first().copied();
    let opposition = participating_factions.get(1).copied();
    let faction_name = &world.factions()[&faction].name;
    let judgment = match intervention.principle {
        Some(ultimate_fate_history::Principle::Duty) if Some(faction) == authority => {
            "We asked for order, and you gave us the means to preserve it. We will be judged by who goes hungry under that order."
        }
        Some(ultimate_fate_history::Principle::Duty) if Some(faction) == opposition => {
            "You preserved the reserve by placing its burden on people who already distrusted the law. Do not mistake silence for consent."
        }
        Some(ultimate_fate_history::Principle::Compassion) if Some(faction) == authority => {
            "The stores are open, and people will eat today. If the next harvest fails, this choice will return with another name."
        }
        Some(ultimate_fate_history::Principle::Compassion) if Some(faction) == opposition => {
            "Opening the stores made the common reserve common again. We support it, though support will not refill an empty granary."
        }
        Some(ultimate_fate_history::Principle::Responsibility) => {
            "The bargain gives each faction something to defend and something to resent. That may be why it has a chance to hold."
        }
        _ => {
            "The decision changed the balance, but no faction agrees yet on what the change means."
        }
    };
    format!(
        "{} treats the intervention as public fact: {}. {judgment}",
        faction_name, intervention.summary
    )
}

fn event_question(world: &HistoricalWorld, event: EventId) -> String {
    let event = &world.events()[&event];
    match event.kind {
        HistoricalEventKind::SettlementFounded => {
            format!("How was {} founded?", world.sites()[&event.location].name)
        }
        HistoricalEventKind::Birth => {
            format!("What birth mattered in year {}?", event.date.year)
        }
        HistoricalEventKind::Death => {
            format!(
                "Who died in year {}, and why does it matter?",
                event.date.year
            )
        }
        HistoricalEventKind::Harvest => {
            format!("What happened during the year {} harvest?", event.date.year)
        }
        HistoricalEventKind::ShortageRecognized => {
            "How did the present shortage begin?".to_string()
        }
        HistoricalEventKind::LawEnacted => "What led to the emergency law?".to_string(),
        HistoricalEventKind::Protest => "Why did people protest?".to_string(),
        HistoricalEventKind::Migration => {
            format!("Why did people move here in year {}?", event.date.year)
        }
        HistoricalEventKind::LeadershipSuccession => {
            "How did the present leadership take power?".to_string()
        }
        HistoricalEventKind::RumorSpread => "How did that story begin?".to_string(),
        HistoricalEventKind::PlayerIntervention => {
            "What changed after the outsider intervened?".to_string()
        }
        HistoricalEventKind::CareDelivered => {
            "How did the patient finally receive treatment?".to_string()
        }
        HistoricalEventKind::ArtifactRecovered => {
            "Who recovered the lost object, and where was it found?".to_string()
        }
        HistoricalEventKind::FormulaReconstructed => {
            "How was the lost magical formula reconstructed?".to_string()
        }
        HistoricalEventKind::DungeonCleared => {
            "What was recovered from beneath the town?".to_string()
        }
        HistoricalEventKind::ProjectPlanned => "Why was this new building proposed?".to_string(),
        HistoricalEventKind::ProjectStalled => {
            "Why did work stop before construction began?".to_string()
        }
        HistoricalEventKind::SupplyShipment => {
            "Who paid to bring the missing supplies into town?".to_string()
        }
        HistoricalEventKind::ProjectStarted => {
            "Who supplied the materials and labor for the new works?".to_string()
        }
        HistoricalEventKind::ProjectCompleted => {
            "What changed when the new works opened?".to_string()
        }
        HistoricalEventKind::ProjectDamaged => "How were the new works damaged?".to_string(),
        HistoricalEventKind::ProjectRepaired => {
            "Why did the sponsors choose to rebuild?".to_string()
        }
        HistoricalEventKind::StrategicBalanceShifted => {
            "How did the wider struggle change?".to_string()
        }
        HistoricalEventKind::RegionalShortage => {
            "Why could local production and trade no longer meet demand?".to_string()
        }
        HistoricalEventKind::RegionalRecovery => {
            "How did the settlement recover from shortage?".to_string()
        }
        HistoricalEventKind::RegionalTrade => {
            "Which settlements depended on this shipment?".to_string()
        }
        HistoricalEventKind::RouteDisrupted => {
            "Why was the regional road no longer safe?".to_string()
        }
        HistoricalEventKind::RouteReopened => {
            "Who restored traffic on the regional road?".to_string()
        }
        HistoricalEventKind::RegionalGoalProposed => {
            "Which faction asked for intervention, and why?".to_string()
        }
        HistoricalEventKind::RegionalGoalResolved => {
            "How did the outsider change the regional situation?".to_string()
        }
        HistoricalEventKind::RegionalPartyArrived => "What did this arrival change?".to_string(),
        HistoricalEventKind::RegionalPartyDefeated => "What led to the road encounter?".to_string(),
    }
}

fn responsibility_question(world: &HistoricalWorld, event: EventId) -> String {
    let event = &world.events()[&event];
    match event.kind {
        HistoricalEventKind::ShortageRecognized => {
            "Who bears responsibility for the shortage?".to_string()
        }
        HistoricalEventKind::Harvest => {
            format!(
                "Who was responsible for the year {} harvest?",
                event.date.year
            )
        }
        HistoricalEventKind::Protest => "Who provoked the protest?".to_string(),
        HistoricalEventKind::LawEnacted => "Who is responsible for the emergency law?".to_string(),
        HistoricalEventKind::PlayerIntervention => {
            "Who benefited from the outsider's intervention?".to_string()
        }
        HistoricalEventKind::DungeonCleared => "Who concealed what was found below?".to_string(),
        HistoricalEventKind::ProjectPlanned
        | HistoricalEventKind::ProjectStalled
        | HistoricalEventKind::SupplyShipment
        | HistoricalEventKind::ProjectStarted
        | HistoricalEventKind::ProjectCompleted
        | HistoricalEventKind::ProjectDamaged
        | HistoricalEventKind::ProjectRepaired
        | HistoricalEventKind::StrategicBalanceShifted => {
            "Who gained from the settlement project?".to_string()
        }
        HistoricalEventKind::RegionalShortage
        | HistoricalEventKind::RegionalRecovery
        | HistoricalEventKind::RegionalTrade
        | HistoricalEventKind::RouteDisrupted
        | HistoricalEventKind::RouteReopened
        | HistoricalEventKind::RegionalGoalProposed
        | HistoricalEventKind::RegionalGoalResolved
        | HistoricalEventKind::RegionalPartyArrived
        | HistoricalEventKind::RegionalPartyDefeated => {
            "Who gained or lost from this regional change?".to_string()
        }
        _ => format!(
            "Who was responsible when {}?",
            event_subject(&event.summary)
        ),
    }
}

fn belief_response(world: &HistoricalWorld, belief: &Belief, proposition: &Proposition) -> String {
    let conviction = match belief.confidence {
        0..=35 => "I have only fragments, but I suspect",
        36..=69 => "I believe",
        _ => "I am convinced",
    };
    let source = match belief.source {
        BeliefSource::Witnessed => "I saw enough of it myself.",
        BeliefSource::ToldBy(person) => {
            return format!(
                "{conviction} {}. {} told me, and I judge the account by what followed.",
                describe_proposition(world, proposition),
                full_name(world, person)
            );
        }
        BeliefSource::FactionDoctrine(faction) => {
            return format!(
                "{conviction} {}. That is the account maintained by {}.",
                describe_proposition(world, proposition),
                world.factions()[&faction].name
            );
        }
    };
    format!(
        "{conviction} {}. {source}",
        describe_proposition(world, proposition)
    )
}

fn describe_proposition(world: &HistoricalWorld, proposition: &Proposition) -> String {
    match proposition {
        Proposition::EventOccurred(event) => {
            format!(
                "the reported event occurred: {}",
                world.events()[event].summary
            )
        }
        Proposition::FactionResponsibleFor { event, faction } => format!(
            "{} bears responsibility for what happened when {}",
            world.factions()[faction].name,
            event_subject(&world.events()[event].summary)
        ),
        Proposition::LawWasNecessary(law) => world
            .sites()
            .values()
            .find_map(|site| site.laws.get(law))
            .map(|law| format!("{} was necessary", law_name(law.kind)))
            .unwrap_or_else(|| "the emergency law was necessary".to_string()),
        Proposition::PersonDied(person) => {
            format!("{} died", full_name(world, *person))
        }
        Proposition::ObjectSurvived(object) => {
            format!("the lost object {} survived", object.0)
        }
        Proposition::FormulaProduces { formula, effect } => format!(
            "{} produces {}",
            formula_name(world, *formula),
            effect.name()
        ),
        Proposition::FormulaRequires { formula, reagent } => format!(
            "{} requires {}",
            formula_name(world, *formula),
            reagent.name()
        ),
    }
}

fn proposition_events(proposition: &Proposition, created_by: EventId) -> Vec<EventId> {
    let mut events = vec![created_by];
    match proposition {
        Proposition::EventOccurred(event) | Proposition::FactionResponsibleFor { event, .. } => {
            events.push(*event);
        }
        Proposition::LawWasNecessary(_)
        | Proposition::PersonDied(_)
        | Proposition::ObjectSurvived(_)
        | Proposition::FormulaProduces { .. }
        | Proposition::FormulaRequires { .. } => {}
    }
    events.sort();
    events.dedup();
    events
}

fn formula_name(world: &HistoricalWorld, formula: ultimate_fate_content::FormulaId) -> String {
    world
        .rules()
        .formula(formula)
        .map(|rule| rule.name.clone())
        .unwrap_or_else(|| format!("formula {}", formula.0))
}

fn event_subject(summary: &str) -> String {
    let subject = summary.trim().trim_end_matches('.');
    let mut characters = subject.chars();
    characters
        .next()
        .map(|first| first.to_lowercase().chain(characters).collect())
        .unwrap_or_default()
}

fn journal_text(
    world: &HistoricalWorld,
    location: SiteId,
    contact: PersonId,
    laws: &[LawId],
) -> String {
    let site = &world.sites()[&location];
    let law_note = if laws.is_empty() {
        "No emergency law was included in the public briefing.".to_string()
    } else {
        format!(
            "Current law: {}.",
            laws.iter()
                .map(|law| law_name(site.laws[law].kind))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "I arrived in {} during a dispute over food and responsibility. {} offered to orient me. {} I should compare official claims with firsthand accounts.",
        site.name,
        full_name(world, contact),
        law_note
    )
}

fn render_briefing(title: &str, paragraphs: &[SourcedParagraph]) -> String {
    let mut output = String::new();
    writeln!(&mut output, "{title}").expect("writing to a string cannot fail");
    for paragraph in paragraphs {
        output.push('\n');
        output.push_str(&paragraph.text);
        output.push('\n');
    }
    output
}

fn full_name(world: &HistoricalWorld, person: PersonId) -> String {
    let person = &world.people()[&person];
    format!(
        "{} {}",
        person.given_name,
        world.families()[&person.family].surname
    )
}

fn law_name(law: LawKind) -> &'static str {
    match law {
        LawKind::FoodRationing => "food rationing",
        LawKind::PriceControls => "price controls",
        LawKind::CompulsoryLabor => "compulsory labor",
        LawKind::PropertySeizure => "property seizure",
        LawKind::Curfew => "a curfew",
        LawKind::OpenGranaries => "open public granaries",
    }
}

fn principle_name(principle: ultimate_fate_history::Principle) -> &'static str {
    use ultimate_fate_history::Principle;
    match principle {
        Principle::Compassion => "compassion",
        Principle::Truth => "truth",
        Principle::Duty => "duty",
        Principle::Freedom => "freedom",
        Principle::Justice => "justice",
        Principle::Responsibility => "responsibility",
        Principle::Courage => "courage",
        Principle::Stewardship => "stewardship",
    }
}

fn occupation_name(occupation: Occupation) -> &'static str {
    match occupation {
        Occupation::Farmer => "farmer",
        Occupation::Miller => "miller",
        Occupation::Merchant => "merchant",
        Occupation::Guard => "guard",
        Occupation::Priest => "priest",
        Occupation::Healer => "healer",
        Occupation::Smith => "smith",
        Occupation::Laborer => "laborer",
        Occupation::Innkeeper => "innkeeper",
        Occupation::Official => "official",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ultimate_fate_history::{CrisisResolutionKind, HistoryEngine};

    fn campaign() -> (HistoryEngine, CampaignStart) {
        let mut history = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        history.simulate_years(20).expect("history should simulate");
        let start = CampaignStart::for_outsider(history.world(), history.primary_site())
            .expect("briefing should build");
        (history, start)
    }

    #[test]
    fn startup_context_is_deterministic() {
        let (_, first) = campaign();
        let (_, second) = campaign();
        assert_eq!(first, second);
    }

    #[test]
    fn briefing_references_only_public_events_and_claims() {
        let (history, start) = campaign();
        for event in start.briefing.referenced_events() {
            assert_eq!(
                history.world().events()[&event].publicity,
                EventPublicity::Public
            );
        }
        for claim in start.briefing.referenced_claims() {
            assert_eq!(
                history.world().claims()[&claim].audience,
                ClaimAudience::Public
            );
        }
    }

    #[test]
    fn briefing_never_labels_public_claims_as_true_or_false() {
        let (_, start) = campaign();
        let text = &start.briefing.rendered_text;
        assert!(!text.contains("TruthValue"));
        assert!(!text.contains("[False]"));
        assert!(!text.contains("[True]"));
        assert!(!text.contains("[Unknown]"));
    }

    #[test]
    fn initial_journal_matches_briefing_knowledge() {
        let (_, start) = campaign();
        assert_eq!(
            start.journal.knowledge.known_events,
            start.briefing.referenced_events()
        );
        assert_eq!(
            start.journal.knowledge.known_claims,
            start.briefing.referenced_claims()
        );
        assert!(!start.hooks.is_empty());
    }

    #[test]
    fn first_evidence_hook_retains_its_historical_event() {
        let (history, start) = campaign();
        let event = start.lead_evidence.expect("lead evidence");
        let site = &history.world().sites()[&start.briefing.location];

        assert!(
            site.physical_evidence
                .iter()
                .any(|evidence| evidence.originating_event == event)
        );
        assert!(
            start
                .hooks
                .iter()
                .any(|hook| hook.target == StoryHookTarget::ExamineEvidence(event))
        );
    }

    #[test]
    fn conversations_are_deterministic_and_only_share_allowed_beliefs() {
        let (history, start) = campaign();
        let context = ConversationContext::default();
        let first =
            Conversation::for_person(history.world(), &start, start.arrival_contact, &context)
                .expect("conversation should build");
        let second =
            Conversation::for_person(history.world(), &start, start.arrival_contact, &context)
                .expect("conversation should build");

        assert_eq!(first, second);
        assert!(!first.topics.is_empty());
        assert!(
            first
                .topics
                .iter()
                .all(|topic| !matches!(topic.kind, ConversationTopicKind::Evidence(_)))
        );
        for topic in &first.topics {
            if let ConversationTopicKind::Claim(claim) = topic.kind {
                assert!(
                    history.world().knowledge().beliefs[&start.arrival_contact][&claim]
                        .willing_to_share
                );
            }
            assert!(!topic.response.contains("TruthValue"));
            assert!(!topic.response.contains("[True]"));
            assert!(!topic.response.contains("[False]"));
        }
    }

    #[test]
    fn examined_evidence_unlocks_a_follow_up_question() {
        let (history, start) = campaign();
        let evidence = start.lead_evidence.expect("lead evidence");
        let mut context = ConversationContext::default();
        context.examined_evidence.insert(evidence);
        let conversation =
            Conversation::for_person(history.world(), &start, start.arrival_contact, &context)
                .expect("conversation should build");

        assert!(
            conversation
                .topics
                .iter()
                .any(|topic| topic.kind == ConversationTopicKind::Evidence(evidence))
        );
    }

    #[test]
    fn factions_react_to_the_recorded_player_intervention() {
        let (mut history, start) = campaign();
        let crisis = start
            .briefing
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.kind == BriefingSectionKind::PresentCrisis)
            .and_then(|paragraph| paragraph.events.first().copied())
            .expect("briefing should reference its crisis");
        let outcome = history
            .resolve_crisis(crisis, CrisisResolutionKind::OpenPublicStores)
            .expect("crisis should resolve");
        let speaker = history
            .world()
            .living_people()
            .find(|person| person.faction == outcome.reaction_faction)
            .map(|person| person.id)
            .expect("reaction faction should have a living member");
        let conversation = Conversation::for_person(
            history.world(),
            &start,
            speaker,
            &ConversationContext::default(),
        )
        .expect("resident should react");
        let reaction = conversation
            .topics
            .iter()
            .find(|topic| topic.kind == ConversationTopicKind::Aftermath(outcome.event))
            .expect("intervention should create a reaction topic");

        assert!(reaction.response.contains(&outcome.summary));
        assert!(
            reaction
                .response
                .contains(&history.world().factions()[&outcome.reaction_faction].name)
        );
    }

    #[test]
    fn many_seeds_produce_safe_startup_context() {
        for seed in 0..32 {
            let mut history = HistoryEngine::seeded_town(seed).expect("town should seed");
            history.simulate_years(30).expect("history should simulate");
            let start = CampaignStart::for_outsider(history.world(), history.primary_site())
                .expect("briefing should build");
            assert!(!start.briefing.rendered_text.is_empty());
            assert!(!start.journal.entries.is_empty());
            let context = ConversationContext::default();
            for person in history.world().living_people() {
                let conversation =
                    Conversation::for_person(history.world(), &start, person.id, &context)
                        .expect("living resident should converse");
                assert!(!conversation.topics.is_empty());
                assert!(conversation.topics.len() <= 7);
            }
        }
    }
}
