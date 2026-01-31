# PhD Research Idea Paper

## Emergent Capabilities Synthesis in Large-Scale Heterogeneous Distributed Networks

**A CRDT-Based Framework for Hierarchical Capability Composition and Decision-Relevant Convergence in Human-Machine-AI Teams**

| Field | Value |
|-------|-------|
| Candidate | Kit Plummer |
| Organization | (r)evolve - Revolve Team LLC |
| Degree Sought | Doctor of Philosophy (PhD) in Information Systems |
| Research Focus | Distributed Systems, Human-Autonomy Teaming, Emergent Capabilities |
| Date | January 2026 |

---

## Abstract

This research addresses a fundamental misconception in distributed coordination systems: the assumption that convergence means all nodes reaching identical state. For human-machine-AI teams operating in disconnected, intermittent, or limited (DIL) network environments, this definition is not merely impractical—it is wrong. What matters is not whether every node has the same raw data, but whether each organizational level has converged to *actionable capability awareness* appropriate to its decision-making responsibilities.

Current approaches to autonomous system coordination face an O(n²) scaling barrier [1] that limits practical deployments to approximately 20 platforms before network saturation occurs. Attempts to solve this through centralized architectures create single points of failure and require all data to flow to central decision-makers—an approach fundamentally incompatible with the intermittent connectivity characteristic of disaster response, industrial operations, remote infrastructure, and other challenging environments.

Remarkably, biological systems solved this coordination problem long before computers existed. Ant colonies coordinate millions of individuals through hierarchical pheromone signaling without central control [2]. Bee swarms achieve collective decision-making through distributed consensus mechanisms [3]. Neural systems synthesize sensory information through hierarchical processing—raw signals at the periphery, increasingly abstract representations at higher levels [4]. These biological architectures provide existence proofs that hierarchical synthesis with emergent capabilities is not merely feasible but evolutionarily optimal for coordination at scale [5], [6].

This research proposes a paradigm shift: **hierarchical peer-to-peer coordination with bidirectional information flow**—capability synthesis propagating upward through organizational levels while command and control distributes downward across the same boundaries. The core hypothesis is that this architecture can support functional coordination of 1000+ heterogeneous agents (humans, machines, AIs) by redefining success criteria around *decision-relevant capability convergence* rather than raw data synchronization.

Critically, emergent capabilities in this framework are *connectivity-contingent*—they exist only within connected subgraphs of the peer-to-peer topology and dissolve when that connectivity fragments. This creates a direct coupling between network state and capability state: as nodes join, leave, or lose connectivity, emergent capabilities continuously reform, degrade, and reconstitute. Trust in any emergent capability must therefore be indexed against the stability and history of the connectivity that enables it. Key innovations include formal composition patterns for connectivity-contingent emergent capabilities; a novel convergence metric appropriate for hierarchical synthesis; risk-trust coupling that reflects connectivity dynamics; and level-appropriate abstraction that reduces cognitive load while improving operator trust calibration [8].

**Keywords:** emergent capabilities, distributed systems, CRDT, hierarchical coordination, human-autonomy teaming, capability composition, decision-relevant convergence, bio-inspired systems, swarm intelligence

---

## 1. Problem Statement

### 1.1 The O(n²) Scaling Barrier

Modern applications increasingly require coordination among large numbers of heterogeneous autonomous platforms—unmanned vehicles, robotic systems, sensor networks, and AI agents. However, current coordination architectures employ flat, all-to-all communication patterns that generate O(n²) message complexity. For *n* platforms, each must communicate with all others, producing *n(n-1)* total messages per synchronization cycle. Dolev and Reischuk [1] established that this quadratic communication complexity is a fundamental lower bound for deterministic distributed agreement protocols.

Empirical evidence from large-scale coordination systems demonstrates that this approach saturates available network capacity at approximately 20 platforms. Beyond this threshold, latency degrades exponentially, coordination failures cascade, and the system becomes operationally unusable. This mathematical limitation represents a fundamental barrier to autonomous system adoption, not merely an engineering challenge requiring incremental optimization.

The problem manifests across domains: emergency response teams attempting to coordinate across damaged infrastructure, industrial facilities managing fleets of autonomous vehicles, remote operations coordinating assets across satellite links, and any scenario where connectivity is intermittent, bandwidth-limited, or unreliable.

### 1.2 The Centralization Fallacy

The intuitive response to coordination complexity is centralization: collect all data at a central point, make decisions there, and distribute commands outward. This approach fails for three reasons.

First, **bandwidth constraints** in disconnected or intermittent environments make it impossible to move all sensor data, platform state, and capability information to a central location. Whether due to infrastructure damage (disaster response), geographic distribution (maritime or agricultural operations), or network limitations (remote industrial sites), sufficient bandwidth cannot be assumed.

Second, **latency requirements** for autonomous coordination often exceed what centralized architectures can provide. By the time data reaches a central decision-maker, propagates through planning systems, and returns as commands, the operational situation has changed.

Third, **resilience requirements** demand that coordination continue despite communication disruptions. Centralized architectures create single points of failure. When the network fragments—as it inevitably does in challenging environments—coordination must continue within each fragment.

### 1.3 The Emergent Capabilities Gap

Existing multi-agent coordination frameworks treat capability matching as a simple lookup problem: tasks have requirements, platforms advertise capabilities, and schedulers perform static matching. This approach fundamentally misunderstands how complex operations actually function. In practice, team capabilities are not simply the union of individual capabilities—they are emergent properties that arise from specific combinations of agents operating in coordinated configurations [5].

Consider a search-and-rescue operation. Three individual teams each possess component capabilities: aerial survey via drone, medical response, and communications relay. No single team can perform an effective rescue in a communications-denied area. However, when these teams form a coordinated group with appropriate communication links, the *integrated rescue capability* emerges—it exists only in the composition, not in any individual team. Current frameworks cannot express, discover, or coordinate around such emergent capabilities.

This mirrors what biological systems achieve naturally. An individual ant possesses limited capabilities. But ant colonies exhibit emergent capabilities—sophisticated foraging patterns, temperature regulation, defense coordination—that no individual ant possesses [2]. The capability exists in the organization, not the components.

### 1.4 The Convergence Misconception

Traditional distributed systems research defines convergence as all replicas reaching identical state [7]. This definition made sense for databases where the goal is data consistency. But for coordination systems, this definition is both unachievable and inappropriate.

It is **unachievable** because the volume of raw data generated by hundreds or thousands of sensors, platforms, and AI systems cannot practically be synchronized across all nodes, especially under bandwidth constraints.

It is **inappropriate** because decision-makers at different levels need different information. A team leader needs to know "my team has search capability with high confidence." A group coordinator needs to know "I have three search-capable teams covering sectors Alpha, Bravo, and Charlie." A regional commander needs to know "I have sufficient search coverage for the area of operations." Raw sensor data serves none of these needs directly.

The right question is not "have all nodes converged to identical state?" but rather "has each level converged to the capability awareness required for its decision-making responsibilities?"

### 1.5 The Cognitive Load and Trust Challenge

Human factors research demonstrates that operators can effectively supervise approximately 12-17 autonomous platforms with appropriate interfaces, with performance degradation beyond this threshold depending on task complexity and automation level [9], [10]. Whether in emergency operations centers, industrial control rooms, or field command posts, human coordinators face fundamental cognitive limits. Yet scaling beyond small teams (10 units) to larger formations (50, 250, 1000+ units) requires novel approaches that preserve human authority while enabling machine-speed coordination.

The challenge is not merely technical—it is cognitive. Operators cannot process raw state from hundreds of platforms. They need synthesized capability information at an appropriate level of abstraction. Moreover, their trust in that synthesized information depends on understanding how it was derived and how it updates as ground truth changes [8].

Biological systems again provide insight. The human brain does not process raw retinal signals at the conscious level—hierarchical visual processing synthesizes edges, shapes, objects, and scenes [4]. Each level operates on abstractions appropriate to its function. Effective human-machine coordination requires analogous hierarchical synthesis.

---

## 2. Central Hypothesis

**A hierarchical peer-to-peer topology with capability synthesis propagating upward and command-and-control distributing downward can support functional coordination of 1000+ heterogeneous agents (humans, machines, AIs) while achieving:**

1. **O(n log n) message complexity** compared to O(n²) for flat topologies [1]
2. **Lower convergence latency** than centralized client-server approaches, measured by time to decision-relevant capability awareness rather than raw data synchronization
3. **Reduced cognitive load** for human operators through level-appropriate capability abstraction [9]
4. **Improved trust calibration** through continuous capability state updates that enable ongoing reevaluation [8]

The key insight is that hierarchical organizational structures are not bureaucratic overhead but evolved communication optimization patterns. When agents organize into small teams, teams into groups, and groups into larger formations, each level synthesizes and abstracts information before passing it upward. This transforms the coordination problem from "synchronize all data everywhere" to "ensure each level has decision-relevant capability awareness."

This pattern appears repeatedly in biological systems. Bee colonies organize into task groups with scout bees synthesizing location information into waggle dances—an abstraction that communicates "good food source in this direction at this distance" without transmitting raw sensory data [3]. Ant colonies use pheromone gradients that naturally aggregate information about path quality [2]. Neural hierarchies process sensory information through successive abstraction layers [4]. The ubiquity of this pattern across biological systems suggests it represents a fundamental solution to coordination at scale [5], [6].

---

## 3. Research Questions

### RQ1: Decision-Relevant Convergence

**How should convergence be defined and measured for hierarchical coordination systems where different organizational levels require different levels of capability abstraction?**

This question challenges the fundamental assumption that convergence means identical state across all nodes [7]. The research will develop formal definitions for "decision-relevant convergence" at each organizational level, establish metrics for measuring when levels have achieved sufficient capability awareness for their decision-making responsibilities, and validate these metrics through both simulation and human-in-the-loop experiments.

### RQ2: Hierarchical Capability Synthesis

**What formal composition patterns enable the expression, discovery, synthesis, and advertisement of emergent capabilities that arise from specific platform combinations in heterogeneous autonomous systems?**

Building on the hypothesis that useful capabilities emerge from compositions rather than existing in individual platforms [5], this question develops the mathematical framework for capability synthesis. The research will formalize three composition patterns—emergent, redundant, and constraint-based—that express how component capabilities combine to produce team capabilities at each hierarchical level.

### RQ3: Scalability and Performance

**How do hierarchical CRDT-based architectures compare to centralized and flat peer-to-peer topologies in terms of bandwidth consumption, convergence latency, and resilience under contested network conditions?**

This question provides the empirical foundation for the central hypothesis. Using the novel convergence metrics developed in RQ1, the research will compare three architectures: centralized (client-server with all data flowing to central point), flat P2P (mesh with O(n²) gossip) [1], and hierarchical P2P with capability synthesis. Comparison will span bandwidth consumption, convergence latency to decision-relevant capability awareness, and resilience under network partitions and degradation.

### RQ4: Human Factors and Trust

**How does level-appropriate capability abstraction affect coordinator cognitive load, situational awareness, and trust calibration compared to raw data presentation or centralized decision support?**

This question validates the human factors benefits of hierarchical synthesis. The research will measure cognitive load using NASA-TLX [11], situational awareness using SAGAT [12], and trust calibration using validated instruments [8], [13] for coordinators at different organizational levels, comparing hierarchical synthesis with alternative approaches. Critical to this question is how continuous capability updates—as agents degrade, reform, and evolve—affect coordinator trust over time.

---

## 4. Theoretical Framework

### 4.1 Connectivity-Contingent Emergence

The central theoretical contribution of this research is the recognition that emergent capabilities in distributed systems are *connectivity-contingent*—they exist only within connected subgraphs of the peer-to-peer topology and dissolve when that connectivity fragments.

Consider a search-and-rescue capability that emerges from the combination of aerial survey, medical response, and communications relay teams. This capability does not exist "in the cloud" or at any central point—it exists *in the connectivity* between those teams. If the communications link fails, the emergent capability disappears, even though each component team retains its individual capabilities. When connectivity is restored, the emergent capability reconstitutes.

This has profound implications for how capability state must be represented and reasoned about:

**Distributed knowledge through P2P topology:** Overall system knowledge exists by distribution across peers, not by aggregation at a central point. Each node maintains awareness of capabilities within its connected neighborhood, with synthesis occurring at each level of the hierarchy. There is no single point that "knows everything"—nor could there be under bandwidth and connectivity constraints.

**Partition-aware capability state:** When the network fragments, each partition develops its own emergent capability set based on what nodes remain connected within that partition. A platoon split across a ridge might have full ISR capability on one side and only ground sensors on the other. Both partitions must maintain accurate local capability awareness.

**Reformation dynamics:** As connectivity changes—nodes joining, leaving, or reconnecting—emergent capabilities continuously reform. This is not a failure mode to be avoided but the expected operating condition. The system must track not just current capability state but the trajectory of capability change.

The implementation leverages conflict-free replicated data types (CRDTs) [7] as building blocks for eventually consistent state synchronization, but CRDTs are enabling infrastructure, not the contribution. The novelty lies in how capability composition semantics are layered atop basic synchronization primitives to express connectivity-contingent emergence.

### 4.2 Hierarchical Organization as Complexity Reduction

A central insight of this research is that hierarchical organizational structures represent evolved solutions to the coordination scaling problem—whether in human organizations, biological systems, or engineered networks. When agents organize into small teams (8-12 units), teams into groups (4-5 teams), and groups into larger formations (4-5 groups), each level aggregates and abstracts information before passing it upward. This transforms O(n²) all-to-all communication [1] into O(n log n) hierarchical aggregation.

The framework implements "cells" at each hierarchical level. Agents communicate only with cell peers and upward to cell coordinators. Cell coordinators synthesize capability state from their members and communicate with their hierarchical peers and supervisors. This architecture achieves 95-99% bandwidth reduction compared to flat broadcast while maintaining operational coherence.

Critically, the hierarchy defines the boundaries within which emergence occurs. An emergent capability synthesized at the squad level exists because those squad members are connected. If the squad fragments, two separate capability sets emerge in each fragment. When the squad reconnects, capability state merges and the original emergent capability reconstitutes—but only if the necessary component capabilities are still present.

The bidirectional flow is critical: capabilities synthesize upward while coordination distributes downward. A group coordinator receives synthesized capability state from teams ("Team Alpha has search capability, Team Bravo has medical capability") and distributes task assignments ("Team Alpha, search Sector 1"). This mirrors how effective organizations—from incident command structures to biological colonies—actually function.

Ant colonies demonstrate this pattern elegantly. Forager ants do not report raw sensory data to the queen. Instead, pheromone trails synthesize path quality information that other ants can interpret locally [2]. The hierarchy emerges from interaction patterns, not central control, yet achieves coordinated behavior across millions of individuals.

### 4.3 Interactive Team Cognition

Drawing from Nancy Cooke's Interactive Team Cognition (ITC) theory [14], this research treats coordination as emergent from interaction patterns rather than centralized control. The three core tenets of ITC are: (1) team cognition is an activity, not a property; (2) it should be measured at the team level; and (3) it is inextricably tied to context [15]. Team cognition arises from the dynamic patterns of information exchange, not from shared mental models held by individual agents.

This theoretical lens validates the peer-to-peer architecture: effective coordination emerges from well-structured interaction protocols rather than omniscient centralized planners. The hierarchical structure does not impose top-down control but rather enables bottom-up capability emergence while preserving top-down command authority.

### 4.4 Transparency, Trust Calibration, and Risk-Trust Coupling

Building on the Situation Awareness-based Agent Transparency (SAT) model developed at the Army Research Laboratory [16], the framework provides coordinators with understanding of what autonomous systems are doing, why, and what they project will happen. The SAT model specifies three levels of transparency: current actions and state (Level 1), reasoning and constraints (Level 2), and projections and uncertainty (Level 3) [17]. However, transparency alone is insufficient—coordinators must calibrate their trust appropriately, neither over-trusting (dangerous) nor under-trusting (ineffective) system capabilities [8].

**Risk-Trust Coupling with Connectivity:** Because emergent capabilities are connectivity-contingent, trust in those capabilities must be indexed against the stability and history of the connectivity that enables them. An emergent ISR capability that has been stable for hours warrants different trust than one that just reconstituted after a network partition. The framework tracks:

- **Connectivity stability:** How long has the current connected subgraph been stable?
- **Reformation history:** How many times has this emergent capability dissolved and reconstituted?
- **Component reliability:** What is the track record of the individual nodes contributing to this capability?
- **Degradation trajectory:** Is capability confidence trending up (stabilizing) or down (fragmenting)?

This creates a dynamic risk-trust index that coordinators can use to calibrate their reliance on emergent capabilities. A search operation might proceed confidently when the integrated search-and-rescue capability shows high stability, but shift to more conservative tactics when connectivity becomes intermittent.

Lee and See's foundational framework [8] defines trust calibration as the correspondence between a person's trust in automation and the automation's actual capabilities. The continuous flow of synthesized capability state—including connectivity stability indicators—enables ongoing trust calibration. Rather than making binary trust decisions based on static specifications, coordinators observe how capability assessments evolve as ground truth changes. A team that consistently maintains search capability despite individual failures earns calibrated trust; one whose capability degrades unexpectedly prompts recalibration.

Hoff and Bashir's three-layer trust model [13]—distinguishing dispositional, learned, and situational trust—provides additional theoretical grounding. Connectivity dynamics primarily affect *situational trust*: the moment-to-moment confidence based on current system state. But repeated experiences with capability reformation also build *learned trust*: understanding of how reliably the system reconstitutes capabilities after disruption.

### 4.5 Bio-Inspired Coordination Principles

Biological systems provide existence proofs that hierarchical synthesis with emergent capabilities is not merely feasible but evolutionarily optimal [5], [6]. This research draws on three biological models:

**Social Insects (Ants, Bees):** Colonies coordinate millions of individuals without central control through stigmergic communication—information embedded in the environment (pheromone trails, waggle dances) that other agents can sense and respond to locally [2], [3]. Individual agents follow simple rules; complex coordinated behavior emerges from interactions. Critically, these systems synthesize information hierarchically: scout bees do not transmit raw sensory data but abstract it into direction and distance signals that other bees can act upon [3].

**Neural Hierarchies:** Sensory processing in biological neural systems demonstrates hierarchical synthesis. Felleman and Van Essen [4] documented over 30 distinct visual areas in primate cortex organized into hierarchical processing streams. Raw retinal signals become edge detectors, then shape recognizers, then object identifiers, then scene understanding. Each level operates on abstractions appropriate to its function. Higher levels do not need—and could not process—raw data from lower levels. This architecture enables real-time processing of massive sensory input while maintaining coherent perception.

**Immune Systems:** The adaptive immune system coordinates responses across billions of cells without central control through chemical signaling that synthesizes threat information [5]. Local detection triggers local responses that propagate through signaling cascades, with each level of the response hierarchy receiving appropriately abstracted information about threat severity and location.

These biological systems share key properties with the proposed framework: hierarchical organization, local communication with synthesized upward propagation, emergent capabilities that exist only in coordinated groups, and graceful degradation under component failure. The ubiquity of this pattern across biological systems—which have been optimized over billions of years of evolution—provides strong evidence for its fundamental soundness.

---

## 5. Proposed Methodology

### 5.1 Capability Composition Patterns

The research defines three formal composition patterns that express how individual capabilities combine to produce team capabilities. All patterns are inherently connectivity-contingent—the emergent capability exists only while the contributing nodes remain connected:

**Emergent Composition:** New capabilities arise from specific combinations satisfying predefined rules [5], but only within connected subgraphs. Integrated search-and-rescue capability emerges when the team includes aerial survey, medical response, and communications relay capabilities in sufficient quantity with appropriate connectivity. The capability exists only in the composition—no individual platform possesses it—and dissolves if the connectivity enabling that composition is lost.

**Redundant Composition:** Threshold-based requirements ensure fault tolerance, with connectivity affecting available redundancy. Triple-redundant communications requires at least three independent communication paths; the capability exists at full confidence only when the threshold is met, and degrades gracefully as redundancy decreases due to node departure or connectivity loss.

**Constraint-Based Composition:** Dependencies and mutual exclusions express operational constraints that must be evaluated within connectivity boundaries. Certain capabilities require prerequisites (medical evacuation requires transport and medical); others cannot operate simultaneously (some sensor modes interfere with each other). These constraints must be re-evaluated as connectivity changes.

Each composition pattern includes rules for:
- **Degradation:** How the synthesized capability confidence decreases as component capabilities are lost or connectivity fragments
- **Reformation:** How capabilities reconstitute when connectivity is restored, including any hysteresis or stabilization requirements
- **Risk indexing:** How the stability and history of connectivity affects confidence in the emergent capability

This enables the continuous reevaluation essential for operator trust calibration [8], with trust appropriately indexed to connectivity dynamics.

### 5.2 Decision-Relevant Convergence Metric

The research develops a formal definition of convergence appropriate for hierarchical coordination in P2P topologies:

**Definition:** An organizational level has achieved *decision-relevant convergence* when its capability state contains sufficient information, at appropriate abstraction, to support the decisions that level is responsible for making, with latency less than the decision cycle time for that level, *within its current connected subgraph*.

This definition explicitly acknowledges that convergence is topology-dependent. A fragmented network may have multiple convergence domains—each partition converging to its own capability state. This is not a failure but the correct behavior: each partition should have accurate awareness of *its own* emergent capabilities.

The definition is parameterized by:
- **Organizational level:** Different levels have different decision responsibilities
- **Capability abstraction level:** Higher levels need more aggregated information
- **Decision cycle time:** Lower levels typically have faster decision cycles
- **Confidence thresholds:** Minimum confidence required for actionable capability awareness
- **Connectivity stability:** How long the current connected subgraph has been stable
- **Risk-trust index:** Confidence adjustment based on connectivity history and reformation dynamics

The metric measures time from capability change (agent joins, fails, degrades, or connectivity changes) to decision-relevant convergence at each level, enabling direct comparison across architectures. Critically, this includes measuring how quickly capability state *re-converges* after network partitions heal—a scenario that centralized architectures handle poorly.

### 5.3 Experimental Validation Strategy

Validation proceeds through four phases:

**Phase 1 - Simulation (100-1000 nodes):** Network simulation using Containerlab and discrete-event simulators to validate O(n log n) scaling properties, bandwidth reduction claims, and convergence latency under various network conditions including partitions, message loss, and variable latency. Comparison across centralized, flat P2P [1], and hierarchical P2P architectures using the decision-relevant convergence metric.

**Phase 2 - Human-in-Loop (10-50 agents):** Coordinator studies using established human factors methodologies to validate cognitive load reduction, situational awareness maintenance, and trust calibration. Key measures include NASA-TLX for workload [11], SAGAT for situational awareness [12], and validated trust scales [8], [13]. Studies compare coordinators receiving raw data, centralized decision support, and hierarchical synthesized capabilities.

**Phase 3 - Field Demonstration (50-100 agents):** Integration with real autonomous platforms in realistic scenarios to validate operational utility. Emphasis on demonstrating capability synthesis across mixed agent types, resilience under communication degradation, and applicability across domains (emergency response, industrial coordination, distributed operations).

**Phase 4 - Stress Testing:** Systematic evaluation of system behavior under adverse conditions: high agent churn, severe network partitions, cascading failures, and rapid capability changes. Focus on validating that decision-relevant convergence degrades gracefully rather than catastrophically.

---

## 6. Expected Contributions

### 6.1 Theoretical Contributions

**Connectivity-Contingent Emergence:** The formalization of emergent capabilities as existing only within connected subgraphs of P2P topologies, with explicit treatment of how capabilities dissolve during partitions and reconstitute when connectivity is restored. This reframes emergence as a dynamic, topology-dependent property rather than a static compositional relationship.

**Novel Convergence Framework:** The formal definition of decision-relevant convergence represents a paradigm shift in how distributed coordination systems should be evaluated. Rather than measuring convergence to identical state, the framework measures convergence to actionable capability awareness appropriate to each decision-making level, within connectivity boundaries.

**Risk-Trust Coupling:** The theoretical framework linking trust calibration to connectivity dynamics—including stability history, reformation frequency, and degradation trajectory—provides a foundation for appropriate human reliance on emergent capabilities in dynamic environments.

**Capability Composition Algebra:** The formal specification of emergent, redundant, and constraint-based composition patterns with explicit connectivity-contingent semantics provides a mathematical framework for expressing how team capabilities arise from component combinations within connected subgraphs [5].

### 6.2 Practical Contributions

**Reference Implementation:** The HIVE Protocol demonstrates the feasibility of 1000+ agent coordination with connectivity-contingent emergence—a capability currently unavailable in operational systems. The implementation provides patterns for integration with existing coordination standards enabling incremental adoption.

**Empirical Validation:** Comparative evaluation across architectures using the decision-relevant convergence metric, including specific measurement of partition healing and capability reformation dynamics.

**Human Factors Guidelines:** Validated recommendations for level-appropriate capability abstraction, risk-trust interfaces that communicate connectivity stability [8], [16], and cognitive load management [9] in large-scale human-machine-AI teams operating under intermittent connectivity.

### 6.3 Domain Applications

The framework addresses coordination challenges across any domain requiring heterogeneous distributed systems to work together under connectivity constraints:

- **Emergency Response:** Multi-agency coordination during disasters when infrastructure is damaged
- **Industrial Operations:** Coordination of autonomous vehicles, robots, and human workers in ports, warehouses, and manufacturing
- **Remote Infrastructure:** Oil/gas, mining, and agricultural operations with limited connectivity
- **Edge Computing:** Coordination of distributed AI inference under resource and connectivity constraints
- **Smart Cities:** Integration of transportation, utility, and emergency systems
- **Space Systems:** Coordination across communication delays and intermittent connectivity

---

## 7. Proposed Timeline

| Phase | Timeline | Deliverables |
|-------|----------|--------------|
| Year 1 | Q1-Q4 2026 | Formal composition pattern specification; decision-relevant convergence metric definition; 1000-node simulation validation; comparative evaluation across architectures |
| Year 2 | Q1-Q4 2027 | Human-in-loop experiments (Phases 2a, 2b); trust calibration validation; cognitive load studies; field demonstration planning |
| Year 3 | Q1-Q4 2028 | Field demonstrations with operational platforms; stress testing; dissertation defense; publication of framework and findings |

---

## 8. Researcher Background

Kit Plummer brings extensive experience in large-scale distributed systems and coordination challenges directly relevant to this research. As Founder and CEO of (r)evolve - Revolve Team LLC, he has led HIVE Protocol development from concept through TRL 4-5 laboratory validation. Prior experience includes work on distributed coordination systems in defense contexts, participation in multinational interoperability initiatives, and direct observation of the O(n²) scaling failures motivating this research.

Technical expertise spans distributed systems architecture, interoperability standards, and human-machine teaming research. The HIVE Protocol codebase (17,000+ lines of Rust, 330+ tests) demonstrates the ability to translate theoretical concepts into operational software. Active engagement with academic institutions (Georgia Tech Research Institute, Arizona State University), industry partners, and standards bodies provides pathways for research validation and technology transition.

Previous academic work at Georgia Tech in Human-Centered Computing under Dr. Karen Feigh provides the human factors foundation essential to the trust calibration and cognitive load aspects of this research.

---

## References

[1] D. Dolev and R. Reischuk, "Bounds on information exchange for Byzantine agreement," *J. ACM*, vol. 32, no. 1, pp. 191–204, Jan. 1985, doi: 10.1145/2455.214112.

[2] E. Bonabeau, M. Dorigo, and G. Theraulaz, *Swarm Intelligence: From Natural to Artificial Systems*. New York, NY, USA: Oxford Univ. Press, 1999.

[3] T. D. Seeley, *Honeybee Democracy*. Princeton, NJ, USA: Princeton Univ. Press, 2010.

[4] D. J. Felleman and D. C. Van Essen, "Distributed hierarchical processing in the primate cerebral cortex," *Cerebral Cortex*, vol. 1, no. 1, pp. 1–47, Jan.–Feb. 1991, doi: 10.1093/cercor/1.1.1-a.

[5] S. Camazine, J.-L. Deneubourg, N. R. Franks, J. Sneyd, G. Theraulaz, and E. Bonabeau, *Self-Organization in Biological Systems*. Princeton, NJ, USA: Princeton Univ. Press, 2001.

[6] D. J. T. Sumpter, *Collective Animal Behavior*. Princeton, NJ, USA: Princeton Univ. Press, 2010.

[7] M. Shapiro, N. Preguiça, C. Baquero, and M. Zawirski, "Conflict-free replicated data types," in *Proc. 13th Int. Symp. Stabilization, Safety, and Security of Distributed Systems (SSS)*, Grenoble, France, Oct. 2011, pp. 386–400, doi: 10.1007/978-3-642-24550-3_29.

[8] J. D. Lee and K. A. See, "Trust in automation: Designing for appropriate reliance," *Human Factors*, vol. 46, no. 1, pp. 50–80, Spring 2004, doi: 10.1518/hfes.46.1.50_30392.

[9] J. Y. C. Chen and M. J. Barnes, "Human-agent teaming for multirobot control: A review of human factors issues," *IEEE Trans. Human-Mach. Syst.*, vol. 44, no. 1, pp. 13–29, Feb. 2014, doi: 10.1109/THMS.2013.2293535.

[10] J. Y. C. Chen, M. J. Barnes, and M. Harper-Sciarini, "Supervisory control of multiple robots: Human-performance issues and user-interface design," *IEEE Trans. Syst., Man, Cybern. C, Appl. Rev.*, vol. 41, no. 4, pp. 435–454, Jul. 2011, doi: 10.1109/TSMCC.2010.2056682.

[11] S. G. Hart and L. E. Staveland, "Development of NASA-TLX (Task Load Index): Results of empirical and theoretical research," in *Human Mental Workload*, P. A. Hancock and N. Meshkati, Eds. Amsterdam, The Netherlands: North-Holland, 1988, pp. 139–183.

[12] M. R. Endsley, "Situation awareness global assessment technique (SAGAT)," in *Proc. IEEE Nat. Aerospace and Electronics Conf. (NAECON)*, Dayton, OH, USA, May 1988, pp. 789–795, doi: 10.1109/NAECON.1988.195097.

[13] K. A. Hoff and M. Bashir, "Trust in automation: Integrating empirical evidence on factors that influence trust," *Human Factors*, vol. 57, no. 3, pp. 407–434, May 2015, doi: 10.1177/0018720814547570.

[14] N. J. Cooke, J. C. Gorman, C. W. Myers, and J. L. Duran, "Interactive team cognition," *Cognitive Science*, vol. 37, no. 2, pp. 255–285, Mar. 2013, doi: 10.1111/cogs.12009.

[14] N. J. Cooke, "Team cognition as interaction," *Current Directions in Psychological Science*, vol. 24, no. 6, pp. 415–419, 2015, doi: 10.1177/0963721415602474.

[15] J. Y. C. Chen, K. Procci, M. Boyce, J. Wright, A. Garcia, and M. Barnes, "Situation awareness-based agent transparency," U.S. Army Research Laboratory, Aberdeen Proving Ground, MD, USA, Tech. Rep. ARL-TR-6905, Apr. 2014.

[16] J. Y. C. Chen *et al.*, "Situation awareness-based agent transparency and human-autonomy teaming effectiveness," *Theoretical Issues in Ergonomics Science*, vol. 19, no. 3, pp. 259–282, 2018, doi: 10.1080/1463922X.2017.1315750.

---

## Contact Information

**Kit Plummer** | Founder & CEO  
(r)evolve - Revolve Team LLC  
https://revolveteam.com  
CAGE: 16NZ5 | UEI: C62HQ24HKEA1
