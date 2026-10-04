namespace DlFovFixer.App.Tests.Support;

/// <summary>Runs code on a single-threaded apartment thread, which WPF rendering needs.</summary>
internal static class Sta
{
    public static T Run<T>(Func<T> action)
    {
        T result = default!;
        Exception? error = null;
        var thread = new Thread(() =>
        {
            try
            {
                result = action();
            }
            catch (Exception exception)
            {
                error = exception;
            }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (error is not null)
        {
            throw new InvalidOperationException("The STA action failed.", error);
        }

        return result;
    }
}
