using System.Runtime.Versioning;
using System.Threading;
using Avalonia.Threading;

namespace Ocy.Desktop.Platform;

public sealed class SingleInstance : IDisposable
{
    private const string MutexName = @"Local\xyz.damt.ocy";
    private const string EventName = @"Local\xyz.damt.ocy.signal";

    private readonly Mutex? _mutex;
    private readonly EventWaitHandle? _eventWaitHandle;
    private RegisteredWaitHandle? _registeredWait;
    private readonly FileStream? _lockFileStream;
    private readonly object _gate = new();
    private bool _disposed;

    private SingleInstance(
        Mutex? mutex,
        EventWaitHandle? eventWaitHandle,
        RegisteredWaitHandle? registeredWait,
        FileStream? lockFileStream)
    {
        _mutex = mutex;
        _eventWaitHandle = eventWaitHandle;
        _registeredWait = registeredWait;
        _lockFileStream = lockFileStream;
    }

    public static SingleInstance? TryAcquire(string dataDir)
    {
        if (OperatingSystem.IsWindows())
        {
            return TryAcquireWindows();
        }

        return TryAcquireUnix(dataDir);
    }

    [SupportedOSPlatform("windows")]
    private static SingleInstance? TryAcquireWindows()
    {
        EventWaitHandle? eventWaitHandle = null;
        try
        {
            eventWaitHandle = new EventWaitHandle(false, EventResetMode.AutoReset, EventName);
        }
        catch
        {
            // Event handle creation may fail; handle below
        }

        Mutex? mutex = null;
        bool createdNew = false;
        try
        {
            mutex = new Mutex(true, MutexName, out createdNew);
        }
        catch
        {
            eventWaitHandle?.Dispose();
            return null;
        }

        if (!createdNew)
        {
            try
            {
                if (eventWaitHandle != null)
                {
                    eventWaitHandle.Set();
                }
                else
                {
                    var deadline = Environment.TickCount64 + 1000;
                    while (Environment.TickCount64 < deadline)
                    {
                        if (EventWaitHandle.TryOpenExisting(EventName, out var existingEvent))
                        {
                            using (existingEvent)
                            {
                                existingEvent.Set();
                            }
                            break;
                        }
                        Thread.Sleep(50);
                    }
                }
            }
            catch
            {
                // Ignore signaling errors
            }
            finally
            {
                eventWaitHandle?.Dispose();
                mutex.Dispose();
            }

            return null;
        }

        if (eventWaitHandle == null)
        {
            try
            {
                eventWaitHandle = new EventWaitHandle(false, EventResetMode.AutoReset, EventName);
            }
            catch
            {
                // Mutex still guards single-instance if wait handle fails
            }
        }

        return new SingleInstance(mutex, eventWaitHandle, null, null);
    }

    public void StartListening(Action onSignal)
    {
        ArgumentNullException.ThrowIfNull(onSignal);
        if (_disposed)
        {
            return;
        }

        if (OperatingSystem.IsWindows())
        {
            lock (_gate)
            {
                if (_registeredWait != null || _eventWaitHandle == null)
                {
                    return;
                }

                try
                {
                    _registeredWait = ThreadPool.RegisterWaitForSingleObject(
                        _eventWaitHandle,
                        (_, timedOut) =>
                        {
                            if (!timedOut)
                            {
                                Dispatcher.UIThread.Post(onSignal);
                            }
                        },
                        null,
                        -1,
                        false);
                }
                catch
                {
                    // Ignore wait registration error
                }
            }
        }
    }

    private static SingleInstance? TryAcquireUnix(string dataDir)
    {
        try
        {
            Directory.CreateDirectory(dataDir);
            var lockFilePath = Path.Combine(dataDir, ".ocy.lock");
            var lockFileStream = new FileStream(
                lockFilePath,
                FileMode.OpenOrCreate,
                FileAccess.ReadWrite,
                FileShare.None);

            return new SingleInstance(null, null, null, lockFileStream);
        }
        catch (IOException)
        {
            return null;
        }
        catch (UnauthorizedAccessException)
        {
            return null;
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;

        if (OperatingSystem.IsWindows())
        {
            lock (_gate)
            {
                _registeredWait?.Unregister(null);
                _registeredWait = null;
            }
            _eventWaitHandle?.Dispose();
            if (_mutex != null)
            {
                try
                {
                    _mutex.ReleaseMutex();
                }
                catch
                {
                    // Ignore if not owned
                }
                _mutex.Dispose();
            }
        }

        _lockFileStream?.Dispose();
    }
}
