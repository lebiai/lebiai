/**
 * Streaming assistant turn — same canvas layout as finished messages.
 * Kept as a thin wrapper so ChatView imports stay stable.
 */
import { MessageBubble, type ToolCallView } from "./MessageBubble";

interface Props {
  text: string;
  thinking: string;
  toolCalls: ToolCallView[];
  /** 组会话里这一轮开口的人（接棒的；没交过棒是第一棒采集）；别的会话为 null。 */
  speaker?: string | null;
}

export function StreamingBubble({ text, thinking, toolCalls, speaker }: Props) {
  return (
    <MessageBubble
      message={{ role: "assistant", content: [] }}
      streaming={{ text, thinking, toolCalls }}
      speaker={speaker}
    />
  );
}
