# **Hybrid AI Strategy for QoreDB**

The objective is to make QoreDB the "Linear for databases" by integrating a context-aware assistant while maintaining a **local-first** and **privacy-focused** architecture.

---

### **1. Three-Tier Model Architecture**

Instead of forcing a single provider, the application supports three flexible ways to power the AI:

- **Local Mode (Privacy-First)**: Connects to local LLMs via **Ollama** or **Llama.cpp**. This ensures 100% data sovereignty and offline capability.
- **BYOK (Bring Your Own Key)**: Allows users to input their own API keys for providers like OpenAI or Anthropic. This delivers high-performance reasoning without requiring a QoreDB cloud account.
- **QoreDB Premium Cloud**: A managed service offering seamless access to top-tier models through a subscription.

### **2. Local Context Injection**

The AI’s value comes from its understanding of the user's specific database.

- **Schema Awareness**: QoreDB uses the **Universal Data Engine** to crawl metadata (tables, columns, types) across SQL and NoSQL sources.
- **Smart Prompting**: Relevant schema parts are injected into the LLM prompt to enable accurate SQL generation and table summaries.
- **Privacy Guard**: Sensitive information is redacted from logs and exports to ensure data privacy.

### **3. Key AI Features**

- **SQL Copilot**: Generates complex queries from natural language and provides auto-completion in the editor.
- **Safety Net**: Analyzes queries for risks, such as `DELETE` commands without `WHERE` clauses, before execution.
- **Schema Architect**: Suggests optimal table structures and index placements based on intended usage.
