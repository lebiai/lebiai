/**
 * Streaming assistant turn — same canvas layout as finished messages.
 * Kept as a thin wrapper so ChatView imports stay stable.
 */
import { MessageBubble, type ToolCallView } from "./MessageBubble";

interface Props {
  text: string;
  thinking: string;
  toolCalls: ToolCallView[];
}

export function StreamingBubble({ text, thinking, toolCalls }: Props) {
  return (
    <MessageBubble
      message={{ role: "assistant", content: [] }}
      streaming={{ text, thinking, toolCalls }}
    />
  );
}
