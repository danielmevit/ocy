namespace Ocy.Desktop.Illustrations;

public sealed class FrameSequencer
{
    public IReadOnlyList<int> Sequence { get; }
    public TimeSpan Step { get; }

    public FrameSequencer(IReadOnlyList<int> sequence, TimeSpan step)
    {
        ArgumentNullException.ThrowIfNull(sequence);
        if (sequence.Count == 0)
        {
            throw new ArgumentException("Sequence cannot be empty.", nameof(sequence));
        }

        Sequence = sequence;
        Step = step > TimeSpan.Zero ? step : TimeSpan.FromMilliseconds(300);
    }

    public int GetFrame(TimeSpan elapsed) => GetFrame(Sequence, Step, elapsed);

    public static int GetFrame(IReadOnlyList<int> sequence, TimeSpan step, TimeSpan elapsed)
    {
        ArgumentNullException.ThrowIfNull(sequence);
        if (sequence.Count == 0)
        {
            throw new ArgumentException("Sequence cannot be empty.", nameof(sequence));
        }

        if (step <= TimeSpan.Zero)
        {
            return sequence[0];
        }

        if (elapsed < TimeSpan.Zero)
        {
            elapsed = TimeSpan.Zero;
        }

        var stepIndex = elapsed.Ticks / step.Ticks;
        var sequenceIndex = (int)(stepIndex % sequence.Count);
        return sequence[sequenceIndex];
    }
}
