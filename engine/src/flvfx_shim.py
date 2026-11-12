# Breakfast Cereal DAW: the module note scripts import as `flvfx`, and the driver the engine talks to.
# Our own code, written from the public description of the scripting interface. The engine sends lines of
# text in and reads lines of text back, so that only a handful of interpreter calls are needed.
import struct
import sys
import types

_insts = {}
_log = []
_KNOB = 262144.0
_PROPS = ('note', 'finePitch', 'velocity', 'pan', 'length', 'output', 'fcut', 'fres', 'color', 'releaseVelocity')


class _Out:
    def write(self, s):
        _log.append(str(s))
        if len(_log) > 400:
            del _log[:200]
        return len(s)

    def flush(self):
        pass


class _Control:
    __slots__ = ('kind', 'name', 'value', 'lo', 'hi', 'opts', 'hint', 'default')

    def __init__(self, kind, name, value, lo, hi, opts, hint):
        self.kind, self.name, self.value, self.lo, self.hi, self.opts, self.hint = kind, name, value, lo, hi, opts, hint
        self.default = value

    def set_normal(self, v):
        v = min(1.0, max(0.0, float(v)))
        if self.kind == 'knob':
            self.value = self.lo + (self.hi - self.lo) * v
        elif self.kind == 'int':
            self.value = int(round(self.lo + (self.hi - self.lo) * v))
        elif self.kind == 'combo':
            self.value = int(round(v * max(0, len(self.opts) - 1)))
        elif self.kind == 'check':
            self.value = 1 if v >= 0.5 else 0
        elif self.kind == 'free':
            self.value = v

    def normal(self):
        try:
            if self.kind in ('knob', 'int'):
                return (self.value - self.lo) / (self.hi - self.lo) if self.hi != self.lo else 0.0
            if self.kind == 'combo':
                return self.value / max(1, len(self.opts) - 1)
            if self.kind in ('check', 'free'):
                return float(self.value)
        except Exception:
            pass
        return 0.0

    def set_stored(self, text):
        # how a preset keeps it: knobs as a share of their travel, the rest as they are
        try:
            if self.kind == 'knob' or self.kind == 'free':
                self.set_normal(int(text) / _KNOB)
            elif self.kind == 'int':
                self.value = int(min(self.hi, max(self.lo, int(text))))
            elif self.kind == 'combo':
                self.value = min(max(0, len(self.opts) - 1), max(0, int(text)))
            elif self.kind == 'check':
                self.value = 1 if int(text) else 0
            else:
                self.value = text
        except Exception:
            pass

    def stored(self):
        if self.kind in ('knob', 'free'):
            return str(int(round(self.normal() * _KNOB)))
        if self.kind == 'text':
            return str(self.value)
        return str(int(self.value))


class ScriptDialog:
    def __init__(self, title='', description='', *more):
        self.title = title
        self.description = description
        self._controls = []
        self._by = {}
        self._group = ''
        self._loose = {}

    def _add(self, kind, name, value, lo=0.0, hi=1.0, opts=None, hint=''):
        full = (self._group + ': ' + str(name)) if self._group else str(name)
        c = _Control(kind, full, value, lo, hi, opts, hint)
        if full in self._by:
            self._controls[self._controls.index(self._by[full])] = c
        else:
            self._controls.append(c)
        self._by[full] = c
        return c

    def addGroup(self, name=''):
        self._group = str(name)

    def endGroup(self):
        self._group = ''

    def addInputKnob(self, name, default=0.0, min=0.0, max=1.0, hint=''):
        self._add('knob', name, float(default), float(min), float(max), None, hint)

    def addInputKnobInt(self, name, default=0, min=0, max=1, hint=''):
        self._add('int', name, int(default), int(min), int(max), None, hint)

    def addInputCheckbox(self, name, default=0, hint=''):
        self._add('check', name, 1 if default else 0, 0, 1, None, hint)

    def addInputCombo(self, name, opts, default=0, hint=''):
        if isinstance(opts, str):
            opts = opts.split(',')
        self._add('combo', name, int(default), 0, 1, [str(o) for o in opts], hint)

    def addInputText(self, name, default='', hint=''):
        self._add('text', name, str(default), 0, 1, None, hint)

    def addInput(self, name, default=0.0, hint=''):
        self._add('free', name, float(default), 0.0, 1.0, None, hint)

    def addInputSurface(self, name='', hint=''):
        # the drawn panel itself is not rebuilt; its controls answer by name from the stored values
        pass

    def _find(self, name):
        name = str(name)
        c = self._by.get(name)
        if c is None:
            tail = ': ' + name
            for k in self._controls:
                if k.name.endswith(tail):
                    return k
        return c

    def getInputValue(self, name):
        c = self._find(name)
        if c is None:
            return self._loose.get(str(name), 0.0)
        return c.value

    def setNormalizedValue(self, name, value):
        c = self._find(name)
        if c is None:
            self._loose[str(name)] = min(1.0, max(0.0, float(value)))
        else:
            c.set_normal(value)

    AddGroup = addGroup
    EndGroup = endGroup
    AddInputKnob = addInputKnob
    AddInputKnobInt = addInputKnobInt
    AddInputCheckbox = addInputCheckbox
    AddInputCombo = addInputCombo
    AddInputText = addInputText
    AddInput = addInput
    AddInputSurface = addInputSurface
    GetInputValue = getInputValue
    SetNormalizedValue = setNormalizedValue


class _Context:
    def __init__(self):
        self._voices = []
        self.ticks = 0
        self.PPQ = 96
        self.isPlaying = False
        self.tempo = 120.0
        self.form = None

    @property
    def voices(self):
        return list(self._voices)

    @property
    def tick(self):
        return self.ticks


def _num(v):
    try:
        v = float(v)
        return v if v == v and abs(v) < 1e9 else 0.0
    except Exception:
        return 0.0


class _Inst:
    def __init__(self, source, stored):
        self.out = []
        self.k = 0
        self.next = 1
        self.incoming = {}
        self.ctx = ctx = _Context()
        self.outs = []
        self.failed = set()
        inst = self

        class Voice:
            def __init__(self, other=None, *more):
                self.note = 60.0
                self.finePitch = 0.0
                self.velocity = 0.8
                self.pan = 0.0
                self.length = 0
                self.output = 0
                self.fcut = 0.0
                self.fres = 0.0
                self.color = 0
                self.releaseVelocity = 0.5
                if other is not None:
                    Voice.copyFrom(self, other)

            def copyFrom(self, other):
                for p in _PROPS:
                    try:
                        setattr(self, p, getattr(other, p))
                    except Exception:
                        pass

            def _state(self):
                return (_num(self.note), _num(self.finePitch), _num(self.velocity), _num(self.pan), int(_num(self.output)), _num(self.fcut), _num(self.fres), int(_num(self.color)) & 15)

            def trigger(self):
                d = self.__dict__
                if d.get('_host') or d.get('_id'):
                    return
                d['_id'] = inst.next
                inst.next += 1
                d['_t0'] = ctx.ticks
                d['_sent'] = self._state()
                ctx._voices.append(self)
                inst.out.append('+ %d %d %r %r %r %r %d %r %r %d' % ((inst.k, d['_id']) + d['_sent']))

            def release(self):
                d = self.__dict__
                i = d.get('_id')
                if not i:
                    return
                d['_id'] = 0
                try:
                    ctx._voices.remove(self)
                except ValueError:
                    pass
                inst.out.append('- %d %d' % (inst.k, i))

        mod = types.ModuleType('flvfx')
        mod.Voice = Voice
        mod.ScriptDialog = ScriptDialog
        mod.context = ctx
        mod.addOutputController = self.add_out
        mod.setOutputController = self.set_out
        self.mod = mod
        self.Voice = Voice
        self.ns = {'__name__': '__note_script__'}
        self.guard(lambda: self.load(source), 'load')
        form = None
        make = self.ns.get('createDialog')
        if callable(make):
            form = self.guard(make, 'createDialog')
        if not isinstance(form, ScriptDialog):
            form = ScriptDialog()
        ctx.form = form
        for line in stored.split('\n'):
            name, eq, value = line.partition('=')
            if eq:
                c = form._find(name)
                if c is not None and c.name == name:
                    c.set_stored(value.strip())
                else:
                    # a control of a drawn panel: kept as the bits of a short float
                    try:
                        x = struct.unpack('<f', struct.pack('<I', int(value) & 0xffffffff))[0]
                        form._loose[name] = x if x == x and abs(x) < 1e9 else 0.0
                    except Exception:
                        pass

    def load(self, source):
        had = sys.modules.get('flvfx')
        sys.modules['flvfx'] = self.mod
        try:
            exec(compile(source, '<note script>', 'exec'), self.ns)
        finally:
            if had is None:
                sys.modules.pop('flvfx', None)
            else:
                sys.modules['flvfx'] = had

    def guard(self, f, what, *a):
        try:
            return f(*a)
        except BaseException as e:
            if what not in self.failed:
                self.failed.add(what)
                self.out.append('! %s: %s: %s' % (what, type(e).__name__, str(e).replace('\n', ' ')[:300]))
            return None

    def add_out(self, name, default=0.0, *more):
        name = str(name)
        for o in self.outs:
            if o[0] == name:
                return
        self.outs.append([name, _num(default), None])

    def set_out(self, name, value, *more):
        name = str(name)
        for o in self.outs:
            if o[0] == name:
                o[1] = _num(value)
                return
        self.outs.append([name, _num(value), None])

    def call(self, name, *a):
        f = self.ns.get(name)
        if callable(f):
            self.guard(f, name, *a)

    def after_tick(self):
        ctx = self.ctx
        for v in list(ctx._voices):
            d = v.__dict__
            try:
                n = int(v.length)
            except Exception:
                n = 0
            if n > 0 and ctx.ticks - d.get('_t0', ctx.ticks) >= n:
                v.release()
                continue
            s = v._state()
            if s != d.get('_sent'):
                d['_sent'] = s
                self.out.append('~ %d %d %r %r %r %r %d %r %r %d' % ((self.k, d['_id']) + s))

    def step(self, text):
        ctx = self.ctx
        self.out = []
        self.k = 0
        old = sys.stdout
        sys.stdout = _OUT
        try:
            for line in text.split('\n'):
                op, _, rest = line.partition(' ')
                if op == 't':
                    for _ in range(int(rest)):
                        ctx.ticks += 1
                        self.k += 1
                        self.call('onTick')
                        self.after_tick()
                elif op == 'c':
                    a = rest.split(' ')
                    ctx.PPQ = int(a[0])
                    ctx.isPlaying = a[1] == '1'
                    ctx.tempo = float(a[2])
                    if len(a) > 3 and a[3] != '-':
                        ctx.ticks = int(a[3])
                elif op == 'n':
                    a = rest.split(' ')
                    v = self.Voice()
                    v.__dict__['_host'] = True
                    v.note, v.velocity, v.pan, v.fcut, v.fres = (float(x) for x in a[1:6])
                    v.color = int(a[6])
                    v.length = int(a[7])
                    self.incoming[a[0]] = v
                    self.call('onTriggerVoice', v)
                elif op == 'r':
                    v = self.incoming.pop(rest, None)
                    if v is not None:
                        self.call('onReleaseVoice', v)
                elif op == 'p':
                    name, _, value = rest.partition('\t')
                    ctx.form.setNormalizedValue(name, float(value))
                elif op == 'x':
                    # everything let go at once
                    for v in list(ctx._voices):
                        v.release()
                    self.incoming.clear()
        finally:
            sys.stdout = old
        for i, o in enumerate(self.outs):
            if o[2] != o[1]:
                o[2] = o[1]
                self.out.append('c %d %r' % (i, o[1]))
        return '\n'.join(self.out)

    def describe(self):
        f = self.ctx.form
        rows = ['d\t%s\t%s' % (str(f.title).replace('\t', ' ').replace('\n', ' '), str(f.description).replace('\t', ' ').replace('\n', ' '))]
        for c in f._controls:
            opts = '|'.join(c.opts) if c.opts else ''
            rows.append('\t'.join(str(x).replace('\t', ' ').replace('\n', ' ') for x in ('k', c.kind, c.name, c.value, c.lo, c.hi, c.normal(), opts, c.hint, c.stored())))
        for o in self.outs:
            rows.append('o\t%s\t%r' % (o[0].replace('\t', ' '), o[1]))
        return '\n'.join(rows)


_OUT = _Out()


def _call(op, a='', b=''):
    """The one door in: `op` is a verb and an instance number."""
    try:
        verb, _, i = op.partition(' ')
        if verb == 'new':
            old = sys.stdout
            sys.stdout = _OUT
            try:
                inst = _Inst(a, b)
            finally:
                sys.stdout = old
            _insts[i] = inst
            return '\n'.join(inst.out)
        if verb == 'drop':
            _insts.pop(i, None)
            return ''
        if verb == 'log':
            s = ''.join(_log)
            del _log[:]
            return s
        inst = _insts.get(i)
        if inst is None:
            return '! no such script'
        if verb == 'step':
            return inst.step(a)
        if verb == 'describe':
            return inst.describe()
        return '! unknown verb'
    except BaseException as e:
        return '! %s: %s' % (type(e).__name__, str(e).replace('\n', ' ')[:300])
