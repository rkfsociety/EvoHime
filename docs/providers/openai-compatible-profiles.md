# OpenAI-compatible provider profiles

`openai_compatible` is the shared chat transport. In provider settings choose
an explicit profile for OpenAI, OpenRouter, Groq, Google Gemini, Mistral,
Cloudflare Workers AI, NVIDIA NIM, Cerebras or Hugging Face Inference Providers.
The profile identifies the vendor independently from the transport. Fixed
profiles use their trusted base URL; only the `custom` profile accepts a
user-supplied HTTPS or loopback URL.

| Profile | Base URL |
| --- | --- |
| OpenAI | `https://api.openai.com/v1` |
| OpenRouter | `https://openrouter.ai/api/v1` |
| Groq | `https://api.groq.com/openai/v1` |
| Google Gemini | `https://generativelanguage.googleapis.com/v1beta/openai` |
| Mistral | `https://api.mistral.ai/v1` |
| Cloudflare Workers AI | Built from the account ID; see below |
| NVIDIA NIM | `https://integrate.api.nvidia.com/v1` |
| Cerebras | `https://api.cerebras.ai/v1` |
| Hugging Face Inference Providers | `https://router.huggingface.co/v1` |

For Cloudflare, enter the 32-character hexadecimal Account ID separately from
the API token. EvoHime builds the account-scoped base URL and model-discovery
request from that ID; the host and path are not editable. For model discovery
and inference, create a token with `Workers AI - Read` and `Workers AI - Edit`
permissions. See Cloudflare's [Workers AI REST API setup instructions](https://developers.cloudflare.com/workers-ai/get-started/rest-api/),
[OpenAI-compatible Workers AI endpoints](https://developers.cloudflare.com/workers-ai/configuration/open-ai-compatibility/)
and [Model Search API](https://developers.cloudflare.com/api/resources/ai/subresources/models/methods/list/).

API credentials remain in the existing encrypted provider store and are sent
to Core through the supervisor environment. The renderer receives profile
identity and bounded model metadata, never the key. If a provider does not
support model discovery, catalog state is shown as `DiscoveryUnsupported`; a
manually entered model ID can still be sent to that selected profile. Missing
capability data remains unknown and is not treated as provider confirmation.
