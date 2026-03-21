# Origins

## Canonical User Inputs (verbatim)

--- 2026-03-21 ---
please read and reread and reread again codex://threads/019d1223-ecf1-7c62-b769-852a60376b66
it is our conversation and full plan for creating jjk - a masterpiece - the future of development
implement it completely from scratch inside codex/jjk_v1 

--- 2026-03-21 ---
let me tell you how i want this to behave, and at the end i dont want you to start working but to brainstorm and chat and plan with me the vision

later ill ask you to create a new project Codex/jjk_v0
treat it as a first class project with repo, git, and jj, product site and hackernews post, and agentic skill for: manually using jjk, knowledge of all functionality, explain jjk to humans, and if asked can automatically use jjk for a project or always


let me tell you how i want this to behave, and at the end i dont want you to start working but to brainstorm and chat and plan with me the vision

later ill ask you to create a new project Codex/jjk_v0
treat it as a first class project with repo, git, and jj, product site and hackernews post, and agentic skill for: manually using jjk, knowledge of all functionality, explain jjk to humans, and if asked can automatically use jjk for a project or always


I want you to think extra hard here, we are designing the future of both human and agentic development
we are creating a new tool called jjk that enables a new kind of dev ux operations using jj and git in the background

This should be so good it superseeds git and jj, 
this should harmonize with git branches and also worktrees, and can use them under the hood

Basics: jjk turns dirs into safe spaces,
Advanced (later): Timeshift to a state across complete terminal state

behavior:

jjk free form desctiptions or info # saves current state with provided input without ""
jjk star # stars current state, can add desc
jjk nice # saves current state as good or improvement
jjk nice description or added information
jjk see # opens a git-graph like branching tree/timeline view of saved states
jjk step # saves the state as a small step, can include desc, this can be set to run automatically when changes happen
jjk up # pushes 
jjk down/pull # fetches updates
jjk return state_name or fuzzy interactive search based on states descriptions
jjk map # finds project dirs, gits, etc
jjk watch # automatically saves steps on changes, (steps can be grouped)
jjk # opens ineractive cli

some more thoughts
every states gets a short uuid, label, desc, datetime, and other usefull metadata

the vision of this is to evolve git
for both humans and agents
higher level states, easy as pie to use, safer development

if the jjk was asked to be used, if the whenever the agents does work, it first makes sure that it is working in a fresh state, meaning if the previous step did not save a state then it does so,
then it does the work it needs to do, and before finishing it saves the state as a group of steps with helpful and relevant info, then if a user likes the change, the agent can apply nice, if the user asks to revert, then it will be easy to return to the last good place

the cool thing is that this can be also integrated with branching messages in converstaions so when you edit or retry something you actually branch not only in the chat but from the state of the files and dirs that were there before
allow for true revert, easy harmless experiments, no more problems when you need to return to a working version, never loose a thing


lets expand the vision, what other good commands should we offer
lets think of this from a top to bottom, meaning thinking of where the rubber hits the road, what is the ux and user stories, which commands can they run and when, whats manual, what is automatic, how does this empower agents and also when working sidebyside humans and agents 

think of this as your nobel award winning contribution for humanity
I want you to think extra hard here, we are designing the future of both human and agentic development
we are creating a new tool called jjk that enables a new kind of dev ux operations using jj and git in the background

This should be so good it superseeds git and jj, 
this should harmonize with git branches and also worktrees, and can use them under the hood

Basics: jjk turns dirs into safe spaces,
Advanced (later): Timeshift to a state across complete terminal state

behavior:

jjk free form desctiptions or info # saves current state with provided input without ""
jjk star # stars current state, can add desc
jjk nice # saves current state as good or improvement
jjk nice description or added information
jjk see # opens a git-graph like branching tree/timeline view of saved states
jjk step # saves the state as a small step, can include desc, this can be set to run automatically when changes happen
jjk up # pushes 
jjk down/pull # fetches updates
jjk return state_name or fuzzy interactive search based on states descriptions
jjk map # finds project dirs, gits, etc
jjk watch # automatically saves steps on changes, (steps can be grouped)
jjk # opens ineractive cli

some more thoughts
every states gets a short uuid, label, desc, datetime, and other usefull metadata

the vision of this is to evolve git
for both humans and agents
higher level states, easy as pie to use, safer development

if the jjk was asked to be used, if the whenever the agents does work, it first makes sure that it is working in a fresh state, meaning if the previous step did not save a state then it does so,
then it does the work it needs to do, and before finishing it saves the state as a group of steps with helpful and relevant info, then if a user likes the change, the agent can apply nice, if the user asks to revert, then it will be easy to return to the last good place

the cool thing is that this can be also integrated with branching messages in converstaions so when you edit or retry something you actually branch not only in the chat but from the state of the files and dirs that were there before
allow for true revert, easy harmless experiments, no more problems when you need to return to a working version, never loose a thing


lets expand the vision, what other good commands should we offer
lets think of this from a top to bottom, meaning thinking of where the rubber hits the road, what is the ux and user stories, which commands can they run and when, whats manual, what is automatic, how does this empower agents and also when working sidebyside humans and agents 

!!! Think of this as your nobel award winning contribution for humanity !!!

--- 2026-03-21 ---
i want you to start working, create the entire jjk in new project folder Codex/jjk_v0

this is your masterpiece, your magnum opus

--- 2026-03-21 ---
please ignore current jjk implementation in Codex/jjk
because that was just a small test which does not include the extensive planning we did

--- 2026-03-21 ---
dont use Codex/jjk at all not even as scaffolding, start from scratch based on our plan

--- 2026-03-21 ---
i want you to start working, create the entire jjk in new project folder Codex/jjk_v0

please ignore current jjk implementation in Codex/jjk
because that was just a small test which does not include the extensive planning we did, dont use Codex/jjk at all not even as scaffolding, start from scratch based on our plan

this is your masterpiece, your magnum opus
