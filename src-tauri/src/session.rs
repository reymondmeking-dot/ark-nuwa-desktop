//! Multi-turn chat session with a distilled agent.
//!
//! A session loads a generated SKILL.md as its system prompt and keeps a
//! rolling message history. This is the "蒸馏后智能体对话状态" — once
//! distillation finishes, the produced skill becomes a live conversational
//! persona.

use crate::llm::ChatMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    /// Human label, usually the distilled person/topic.
    pub title: String,
    /// The SKILL.md content used as the system prompt.
    pub system_prompt: String,
    /// Rolling conversation history (excludes the system prompt).
    pub history: Vec<ChatMessage>,
}

impl ChatSession {
    pub fn new(id: String, title: String, skill_markdown: String) -> Self {
        let system_prompt = format!(
            "你将以下面这份「视角 Skill」所描述的认知框架来回答。\
             严格遵守其中的能力边界：遇到未知问题要诚实表达不确定，不要编造。\n\n{skill_markdown}"
        );
        Self {
            id,
            title,
            system_prompt,
            history: Vec::new(),
        }
    }

    /// Record a user turn.
    pub fn push_user(&mut self, content: impl Into<String>) {
        self.history.push(ChatMessage::user(content));
    }

    /// Record an assistant turn.
    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.history.push(ChatMessage::assistant(content));
    }

    /// Full message list for an API call: system prompt + history.
    pub fn messages(&self) -> Vec<ChatMessage> {
        let mut msgs = Vec::with_capacity(self.history.len() + 1);
        msgs.push(ChatMessage::system(self.system_prompt.clone()));
        msgs.extend(self.history.iter().cloned());
        msgs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_message_list_with_system_first() {
        let mut s = ChatSession::new("1".into(), "段永平".into(), "# skill".into());
        s.push_user("怎么看待长期主义？");
        s.push_assistant("本分。");
        let msgs = s.messages();
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs.len(), 3);
        assert!(msgs[0].content.contains("# skill"));
    }
}
