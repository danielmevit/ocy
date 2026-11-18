using Ocy.Core.Exercises;
using Xunit;

namespace Ocy.Core.Tests;

public sealed class ExercisePickerTests
{
    [Fact]
    public void Catalogue_ContainsExpectedInitialExercises()
    {
        Assert.Equal(12, ExerciseCatalogue.All.Count);
        Assert.Equal(9, ExerciseCatalogue.All.Count(e => e.Kind == ExerciseKind.Eyes));
        Assert.Equal(3, ExerciseCatalogue.All.Count(e => e.Kind == ExerciseKind.Body));

        var windowFar = ExerciseCatalogue.ById["window-far"];
        Assert.True(windowFar.RequiresWindow);
        Assert.Equal("exercise.window-far.title", windowFar.TitleKey);
        Assert.Equal("exercise.window-far.body", windowFar.BodyKey);
    }

    [Fact]
    public void PickShortExercise_AlwaysEyesOnly()
    {
        var picker = new ExercisePicker(new Random(12345));

        for (int i = 0; i < 50; i++)
        {
            var exercise = picker.PickShortExercise(hasWindowNearby: true);
            Assert.Equal(ExerciseKind.Eyes, exercise.Kind);
        }
    }

    [Fact]
    public void WindowFilter_ExcludesWindowExercisesWhenNoWindowNearby()
    {
        var picker = new ExercisePicker(new Random(42));

        for (int i = 0; i < 100; i++)
        {
            var exercise = picker.PickShortExercise(hasWindowNearby: false);
            Assert.False(exercise.RequiresWindow);
            Assert.NotEqual("window-far", exercise.Id);
        }
    }

    [Fact]
    public void NoImmediateRepeat_AvoidsConsecutiveIdenticalIds()
    {
        var picker = new ExercisePicker(new Random(999));
        string? previousId = null;

        for (int i = 0; i < 100; i++)
        {
            var exercise = picker.PickShortExercise(hasWindowNearby: true, previousExerciseId: previousId);
            Assert.NotEqual(previousId, exercise.Id);
            previousId = exercise.Id;
        }
    }

    [Fact]
    public void DeterministicWithSeededRandom()
    {
        var picker1 = new ExercisePicker(new Random(777));
        var picker2 = new ExercisePicker(new Random(777));

        for (int i = 0; i < 20; i++)
        {
            var ex1 = picker1.PickShortExercise(hasWindowNearby: true);
            var ex2 = picker2.PickShortExercise(hasWindowNearby: true);
            Assert.Equal(ex1.Id, ex2.Id);
        }
    }

    [Fact]
    public void PickLongExercises_PicksOneBodyAndOneEyes()
    {
        var picker = new ExercisePicker(new Random(555));

        for (int i = 0; i < 30; i++)
        {
            var (body, eyes) = picker.PickLongExercises(hasWindowNearby: true);
            Assert.Equal(ExerciseKind.Body, body.Kind);
            Assert.Equal(ExerciseKind.Eyes, eyes.Kind);
        }
    }
}
