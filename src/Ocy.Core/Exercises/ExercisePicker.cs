namespace Ocy.Core.Exercises;

public sealed class ExercisePicker : IExercisePicker
{
    private readonly Random _random;

    public ExercisePicker(Random? random = null)
    {
        _random = random ?? Random.Shared;
    }

    public ExerciseDefinition PickShortExercise(bool hasWindowNearby, string? previousExerciseId = null)
    {
        var candidates = ExerciseCatalogue.All
            .Where(e => e.Kind == ExerciseKind.Eyes)
            .Where(e => hasWindowNearby || !e.RequiresWindow)
            .ToList();

        if (candidates.Count > 1 && !string.IsNullOrEmpty(previousExerciseId))
        {
            var withoutPrevious = candidates.Where(e => !string.Equals(e.Id, previousExerciseId, StringComparison.OrdinalIgnoreCase)).ToList();
            if (withoutPrevious.Count > 0)
            {
                candidates = withoutPrevious;
            }
        }

        int index = _random.Next(candidates.Count);
        return candidates[index];
    }

    public (ExerciseDefinition Body, ExerciseDefinition Eyes) PickLongExercises(
        bool hasWindowNearby,
        string? previousEyesId = null,
        string? previousBodyId = null)
    {
        // 1. Pick Body
        var bodyCandidates = ExerciseCatalogue.All
            .Where(e => e.Kind == ExerciseKind.Body)
            .Where(e => hasWindowNearby || !e.RequiresWindow)
            .ToList();

        if (bodyCandidates.Count > 1 && !string.IsNullOrEmpty(previousBodyId))
        {
            var withoutPrevious = bodyCandidates.Where(e => !string.Equals(e.Id, previousBodyId, StringComparison.OrdinalIgnoreCase)).ToList();
            if (withoutPrevious.Count > 0)
            {
                bodyCandidates = withoutPrevious;
            }
        }

        var body = bodyCandidates[_random.Next(bodyCandidates.Count)];

        // 2. Pick Eyes
        var eyes = PickShortExercise(hasWindowNearby, previousEyesId);

        return (body, eyes);
    }
}
