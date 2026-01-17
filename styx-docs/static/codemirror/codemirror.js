let Qo = [], wf = [];
(() => {
  let n = "lc,34,7n,7,7b,19,,,,2,,2,,,20,b,1c,l,g,,2t,7,2,6,2,2,,4,z,,u,r,2j,b,1m,9,9,,o,4,,9,,3,,5,17,3,3b,f,,w,1j,,,,4,8,4,,3,7,a,2,t,,1m,,,,2,4,8,,9,,a,2,q,,2,2,1l,,4,2,4,2,2,3,3,,u,2,3,,b,2,1l,,4,5,,2,4,,k,2,m,6,,,1m,,,2,,4,8,,7,3,a,2,u,,1n,,,,c,,9,,14,,3,,1l,3,5,3,,4,7,2,b,2,t,,1m,,2,,2,,3,,5,2,7,2,b,2,s,2,1l,2,,,2,4,8,,9,,a,2,t,,20,,4,,2,3,,,8,,29,,2,7,c,8,2q,,2,9,b,6,22,2,r,,,,,,1j,e,,5,,2,5,b,,10,9,,2u,4,,6,,2,2,2,p,2,4,3,g,4,d,,2,2,6,,f,,jj,3,qa,3,t,3,t,2,u,2,1s,2,,7,8,,2,b,9,,19,3,3b,2,y,,3a,3,4,2,9,,6,3,63,2,2,,1m,,,7,,,,,2,8,6,a,2,,1c,h,1r,4,1c,7,,,5,,14,9,c,2,w,4,2,2,,3,1k,,,2,3,,,3,1m,8,2,2,48,3,,d,,7,4,,6,,3,2,5i,1m,,5,ek,,5f,x,2da,3,3x,,2o,w,fe,6,2x,2,n9w,4,,a,w,2,28,2,7k,,3,,4,,p,2,5,,47,2,q,i,d,,12,8,p,b,1a,3,1c,,2,4,2,2,13,,1v,6,2,2,2,2,c,,8,,1b,,1f,,,3,2,2,5,2,,,16,2,8,,6m,,2,,4,,fn4,,kh,g,g,g,a6,2,gt,,6a,,45,5,1ae,3,,2,5,4,14,3,4,,4l,2,fx,4,ar,2,49,b,4w,,1i,f,1k,3,1d,4,2,2,1x,3,10,5,,8,1q,,c,2,1g,9,a,4,2,,2n,3,2,,,2,6,,4g,,3,8,l,2,1l,2,,,,,m,,e,7,3,5,5f,8,2,3,,,n,,29,,2,6,,,2,,,2,,2,6j,,2,4,6,2,,2,r,2,2d,8,2,,,2,2y,,,,2,6,,,2t,3,2,4,,5,77,9,,2,6t,,a,2,,,4,,40,4,2,2,4,,w,a,14,6,2,4,8,,9,6,2,3,1a,d,,2,ba,7,,6,,,2a,m,2,7,,2,,2,3e,6,3,,,2,,7,,,20,2,3,,,,9n,2,f0b,5,1n,7,t4,,1r,4,29,,f5k,2,43q,,,3,4,5,8,8,2,7,u,4,44,3,1iz,1j,4,1e,8,,e,,m,5,,f,11s,7,,h,2,7,,2,,5,79,7,c5,4,15s,7,31,7,240,5,gx7k,2o,3k,6o".split(",").map((e) => e ? parseInt(e, 36) : 1);
  for (let e = 0, t = 0; e < n.length; e++)
    (e % 2 ? wf : Qo).push(t = t + n[e]);
})();
function Pg(n) {
  if (n < 768) return !1;
  for (let e = 0, t = Qo.length; ; ) {
    let i = e + t >> 1;
    if (n < Qo[i]) t = i;
    else if (n >= wf[i]) e = i + 1;
    else return !0;
    if (e == t) return !1;
  }
}
function ch(n) {
  return n >= 127462 && n <= 127487;
}
const fh = 8205;
function Rg(n, e, t = !0, i = !0) {
  return (t ? Sf : Lg)(n, e, i);
}
function Sf(n, e, t) {
  if (e == n.length) return e;
  e && Cf(n.charCodeAt(e)) && Of(n.charCodeAt(e - 1)) && e--;
  let i = oo(n, e);
  for (e += uh(i); e < n.length; ) {
    let r = oo(n, e);
    if (i == fh || r == fh || t && Pg(r))
      e += uh(r), i = r;
    else if (ch(r)) {
      let s = 0, o = e - 2;
      for (; o >= 0 && ch(oo(n, o)); )
        s++, o -= 2;
      if (s % 2 == 0) break;
      e += 2;
    } else
      break;
  }
  return e;
}
function Lg(n, e, t) {
  for (; e > 0; ) {
    let i = Sf(n, e - 2, t);
    if (i < e) return i;
    e--;
  }
  return 0;
}
function oo(n, e) {
  let t = n.charCodeAt(e);
  if (!Of(t) || e + 1 == n.length) return t;
  let i = n.charCodeAt(e + 1);
  return Cf(i) ? (t - 55296 << 10) + (i - 56320) + 65536 : t;
}
function Cf(n) {
  return n >= 56320 && n < 57344;
}
function Of(n) {
  return n >= 55296 && n < 56320;
}
function uh(n) {
  return n < 65536 ? 1 : 2;
}
let ge = class Af {
  /**
  Get the line description around the given position.
  */
  lineAt(e) {
    if (e < 0 || e > this.length)
      throw new RangeError(`Invalid position ${e} in document of length ${this.length}`);
    return this.lineInner(e, !1, 1, 0);
  }
  /**
  Get the description for the given (1-based) line number.
  */
  line(e) {
    if (e < 1 || e > this.lines)
      throw new RangeError(`Invalid line number ${e} in ${this.lines}-line document`);
    return this.lineInner(e, !0, 1, 0);
  }
  /**
  Replace a range of the text with the given content.
  */
  replace(e, t, i) {
    [e, t] = vn(this, e, t);
    let r = [];
    return this.decompose(
      0,
      e,
      r,
      2
      /* Open.To */
    ), i.length && i.decompose(
      0,
      i.length,
      r,
      3
      /* Open.To */
    ), this.decompose(
      t,
      this.length,
      r,
      1
      /* Open.From */
    ), Xt.from(r, this.length - (t - e) + i.length);
  }
  /**
  Append another document to this one.
  */
  append(e) {
    return this.replace(this.length, this.length, e);
  }
  /**
  Retrieve the text between the given points.
  */
  slice(e, t = this.length) {
    [e, t] = vn(this, e, t);
    let i = [];
    return this.decompose(e, t, i, 0), Xt.from(i, t - e);
  }
  /**
  Test whether this text is equal to another instance.
  */
  eq(e) {
    if (e == this)
      return !0;
    if (e.length != this.length || e.lines != this.lines)
      return !1;
    let t = this.scanIdentical(e, 1), i = this.length - this.scanIdentical(e, -1), r = new $n(this), s = new $n(e);
    for (let o = t, l = t; ; ) {
      if (r.next(o), s.next(o), o = 0, r.lineBreak != s.lineBreak || r.done != s.done || r.value != s.value)
        return !1;
      if (l += r.value.length, r.done || l >= i)
        return !0;
    }
  }
  /**
  Iterate over the text. When `dir` is `-1`, iteration happens
  from end to start. This will return lines and the breaks between
  them as separate strings.
  */
  iter(e = 1) {
    return new $n(this, e);
  }
  /**
  Iterate over a range of the text. When `from` > `to`, the
  iterator will run in reverse.
  */
  iterRange(e, t = this.length) {
    return new Mf(this, e, t);
  }
  /**
  Return a cursor that iterates over the given range of lines,
  _without_ returning the line breaks between, and yielding empty
  strings for empty lines.
  
  When `from` and `to` are given, they should be 1-based line numbers.
  */
  iterLines(e, t) {
    let i;
    if (e == null)
      i = this.iter();
    else {
      t == null && (t = this.lines + 1);
      let r = this.line(e).from;
      i = this.iterRange(r, Math.max(r, t == this.lines + 1 ? this.length : t <= 1 ? 0 : this.line(t - 1).to));
    }
    return new Tf(i);
  }
  /**
  Return the document as a string, using newline characters to
  separate lines.
  */
  toString() {
    return this.sliceString(0);
  }
  /**
  Convert the document to an array of lines (which can be
  deserialized again via [`Text.of`](https://codemirror.net/6/docs/ref/#state.Text^of)).
  */
  toJSON() {
    let e = [];
    return this.flatten(e), e;
  }
  /**
  @internal
  */
  constructor() {
  }
  /**
  Create a `Text` instance for the given array of lines.
  */
  static of(e) {
    if (e.length == 0)
      throw new RangeError("A document must have at least one line");
    return e.length == 1 && !e[0] ? Af.empty : e.length <= 32 ? new Le(e) : Xt.from(Le.split(e, []));
  }
};
class Le extends ge {
  constructor(e, t = Dg(e)) {
    super(), this.text = e, this.length = t;
  }
  get lines() {
    return this.text.length;
  }
  get children() {
    return null;
  }
  lineInner(e, t, i, r) {
    for (let s = 0; ; s++) {
      let o = this.text[s], l = r + o.length;
      if ((t ? i : l) >= e)
        return new Bg(r, l, i, o);
      r = l + 1, i++;
    }
  }
  decompose(e, t, i, r) {
    let s = e <= 0 && t >= this.length ? this : new Le(dh(this.text, e, t), Math.min(t, this.length) - Math.max(0, e));
    if (r & 1) {
      let o = i.pop(), l = Kr(s.text, o.text.slice(), 0, s.length);
      if (l.length <= 32)
        i.push(new Le(l, o.length + s.length));
      else {
        let a = l.length >> 1;
        i.push(new Le(l.slice(0, a)), new Le(l.slice(a)));
      }
    } else
      i.push(s);
  }
  replace(e, t, i) {
    if (!(i instanceof Le))
      return super.replace(e, t, i);
    [e, t] = vn(this, e, t);
    let r = Kr(this.text, Kr(i.text, dh(this.text, 0, e)), t), s = this.length + i.length - (t - e);
    return r.length <= 32 ? new Le(r, s) : Xt.from(Le.split(r, []), s);
  }
  sliceString(e, t = this.length, i = `
`) {
    [e, t] = vn(this, e, t);
    let r = "";
    for (let s = 0, o = 0; s <= t && o < this.text.length; o++) {
      let l = this.text[o], a = s + l.length;
      s > e && o && (r += i), e < a && t > s && (r += l.slice(Math.max(0, e - s), t - s)), s = a + 1;
    }
    return r;
  }
  flatten(e) {
    for (let t of this.text)
      e.push(t);
  }
  scanIdentical() {
    return 0;
  }
  static split(e, t) {
    let i = [], r = -1;
    for (let s of e)
      i.push(s), r += s.length + 1, i.length == 32 && (t.push(new Le(i, r)), i = [], r = -1);
    return r > -1 && t.push(new Le(i, r)), t;
  }
}
class Xt extends ge {
  constructor(e, t) {
    super(), this.children = e, this.length = t, this.lines = 0;
    for (let i of e)
      this.lines += i.lines;
  }
  lineInner(e, t, i, r) {
    for (let s = 0; ; s++) {
      let o = this.children[s], l = r + o.length, a = i + o.lines - 1;
      if ((t ? a : l) >= e)
        return o.lineInner(e, t, i, r);
      r = l + 1, i = a + 1;
    }
  }
  decompose(e, t, i, r) {
    for (let s = 0, o = 0; o <= t && s < this.children.length; s++) {
      let l = this.children[s], a = o + l.length;
      if (e <= a && t >= o) {
        let f = r & ((o <= e ? 1 : 0) | (a >= t ? 2 : 0));
        o >= e && a <= t && !f ? i.push(l) : l.decompose(e - o, t - o, i, f);
      }
      o = a + 1;
    }
  }
  replace(e, t, i) {
    if ([e, t] = vn(this, e, t), i.lines < this.lines)
      for (let r = 0, s = 0; r < this.children.length; r++) {
        let o = this.children[r], l = s + o.length;
        if (e >= s && t <= l) {
          let a = o.replace(e - s, t - s, i), f = this.lines - o.lines + a.lines;
          if (a.lines < f >> 4 && a.lines > f >> 6) {
            let u = this.children.slice();
            return u[r] = a, new Xt(u, this.length - (t - e) + i.length);
          }
          return super.replace(s, l, a);
        }
        s = l + 1;
      }
    return super.replace(e, t, i);
  }
  sliceString(e, t = this.length, i = `
`) {
    [e, t] = vn(this, e, t);
    let r = "";
    for (let s = 0, o = 0; s < this.children.length && o <= t; s++) {
      let l = this.children[s], a = o + l.length;
      o > e && s && (r += i), e < a && t > o && (r += l.sliceString(e - o, t - o, i)), o = a + 1;
    }
    return r;
  }
  flatten(e) {
    for (let t of this.children)
      t.flatten(e);
  }
  scanIdentical(e, t) {
    if (!(e instanceof Xt))
      return 0;
    let i = 0, [r, s, o, l] = t > 0 ? [0, 0, this.children.length, e.children.length] : [this.children.length - 1, e.children.length - 1, -1, -1];
    for (; ; r += t, s += t) {
      if (r == o || s == l)
        return i;
      let a = this.children[r], f = e.children[s];
      if (a != f)
        return i + a.scanIdentical(f, t);
      i += a.length + 1;
    }
  }
  static from(e, t = e.reduce((i, r) => i + r.length + 1, -1)) {
    let i = 0;
    for (let b of e)
      i += b.lines;
    if (i < 32) {
      let b = [];
      for (let k of e)
        k.flatten(b);
      return new Le(b, t);
    }
    let r = Math.max(
      32,
      i >> 5
      /* Tree.BranchShift */
    ), s = r << 1, o = r >> 1, l = [], a = 0, f = -1, u = [];
    function g(b) {
      let k;
      if (b.lines > s && b instanceof Xt)
        for (let C of b.children)
          g(C);
      else b.lines > o && (a > o || !a) ? (y(), l.push(b)) : b instanceof Le && a && (k = u[u.length - 1]) instanceof Le && b.lines + k.lines <= 32 ? (a += b.lines, f += b.length + 1, u[u.length - 1] = new Le(k.text.concat(b.text), k.length + 1 + b.length)) : (a + b.lines > r && y(), a += b.lines, f += b.length + 1, u.push(b));
    }
    function y() {
      a != 0 && (l.push(u.length == 1 ? u[0] : Xt.from(u, f)), f = -1, a = u.length = 0);
    }
    for (let b of e)
      g(b);
    return y(), l.length == 1 ? l[0] : new Xt(l, t);
  }
}
ge.empty = /* @__PURE__ */ new Le([""], 0);
function Dg(n) {
  let e = -1;
  for (let t of n)
    e += t.length + 1;
  return e;
}
function Kr(n, e, t = 0, i = 1e9) {
  for (let r = 0, s = 0, o = !0; s < n.length && r <= i; s++) {
    let l = n[s], a = r + l.length;
    a >= t && (a > i && (l = l.slice(0, i - r)), r < t && (l = l.slice(t - r)), o ? (e[e.length - 1] += l, o = !1) : e.push(l)), r = a + 1;
  }
  return e;
}
function dh(n, e, t) {
  return Kr(n, [""], e, t);
}
class $n {
  constructor(e, t = 1) {
    this.dir = t, this.done = !1, this.lineBreak = !1, this.value = "", this.nodes = [e], this.offsets = [t > 0 ? 1 : (e instanceof Le ? e.text.length : e.children.length) << 1];
  }
  nextInner(e, t) {
    for (this.done = this.lineBreak = !1; ; ) {
      let i = this.nodes.length - 1, r = this.nodes[i], s = this.offsets[i], o = s >> 1, l = r instanceof Le ? r.text.length : r.children.length;
      if (o == (t > 0 ? l : 0)) {
        if (i == 0)
          return this.done = !0, this.value = "", this;
        t > 0 && this.offsets[i - 1]++, this.nodes.pop(), this.offsets.pop();
      } else if ((s & 1) == (t > 0 ? 0 : 1)) {
        if (this.offsets[i] += t, e == 0)
          return this.lineBreak = !0, this.value = `
`, this;
        e--;
      } else if (r instanceof Le) {
        let a = r.text[o + (t < 0 ? -1 : 0)];
        if (this.offsets[i] += t, a.length > Math.max(0, e))
          return this.value = e == 0 ? a : t > 0 ? a.slice(e) : a.slice(0, a.length - e), this;
        e -= a.length;
      } else {
        let a = r.children[o + (t < 0 ? -1 : 0)];
        e > a.length ? (e -= a.length, this.offsets[i] += t) : (t < 0 && this.offsets[i]--, this.nodes.push(a), this.offsets.push(t > 0 ? 1 : (a instanceof Le ? a.text.length : a.children.length) << 1));
      }
    }
  }
  next(e = 0) {
    return e < 0 && (this.nextInner(-e, -this.dir), e = this.value.length), this.nextInner(e, this.dir);
  }
}
class Mf {
  constructor(e, t, i) {
    this.value = "", this.done = !1, this.cursor = new $n(e, t > i ? -1 : 1), this.pos = t > i ? e.length : 0, this.from = Math.min(t, i), this.to = Math.max(t, i);
  }
  nextInner(e, t) {
    if (t < 0 ? this.pos <= this.from : this.pos >= this.to)
      return this.value = "", this.done = !0, this;
    e += Math.max(0, t < 0 ? this.pos - this.to : this.from - this.pos);
    let i = t < 0 ? this.pos - this.from : this.to - this.pos;
    e > i && (e = i), i -= e;
    let { value: r } = this.cursor.next(e);
    return this.pos += (r.length + e) * t, this.value = r.length <= i ? r : t < 0 ? r.slice(r.length - i) : r.slice(0, i), this.done = !this.value, this;
  }
  next(e = 0) {
    return e < 0 ? e = Math.max(e, this.from - this.pos) : e > 0 && (e = Math.min(e, this.to - this.pos)), this.nextInner(e, this.cursor.dir);
  }
  get lineBreak() {
    return this.cursor.lineBreak && this.value != "";
  }
}
class Tf {
  constructor(e) {
    this.inner = e, this.afterBreak = !0, this.value = "", this.done = !1;
  }
  next(e = 0) {
    let { done: t, lineBreak: i, value: r } = this.inner.next(e);
    return t && this.afterBreak ? (this.value = "", this.afterBreak = !1) : t ? (this.done = !0, this.value = "") : i ? this.afterBreak ? this.value = "" : (this.afterBreak = !0, this.next()) : (this.value = r, this.afterBreak = !1), this;
  }
  get lineBreak() {
    return !1;
  }
}
typeof Symbol < "u" && (ge.prototype[Symbol.iterator] = function() {
  return this.iter();
}, $n.prototype[Symbol.iterator] = Mf.prototype[Symbol.iterator] = Tf.prototype[Symbol.iterator] = function() {
  return this;
});
class Bg {
  /**
  @internal
  */
  constructor(e, t, i, r) {
    this.from = e, this.to = t, this.number = i, this.text = r;
  }
  /**
  The length of the line (not including any line break after it).
  */
  get length() {
    return this.to - this.from;
  }
}
function vn(n, e, t) {
  return e = Math.max(0, Math.min(n.length, e)), [e, Math.max(e, Math.min(n.length, t))];
}
function We(n, e, t = !0, i = !0) {
  return Rg(n, e, t, i);
}
function Eg(n) {
  return n >= 56320 && n < 57344;
}
function Ig(n) {
  return n >= 55296 && n < 56320;
}
function at(n, e) {
  let t = n.charCodeAt(e);
  if (!Ig(t) || e + 1 == n.length)
    return t;
  let i = n.charCodeAt(e + 1);
  return Eg(i) ? (t - 55296 << 10) + (i - 56320) + 65536 : t;
}
function Ql(n) {
  return n <= 65535 ? String.fromCharCode(n) : (n -= 65536, String.fromCharCode((n >> 10) + 55296, (n & 1023) + 56320));
}
function Yt(n) {
  return n < 65536 ? 1 : 2;
}
const Vo = /\r\n?|\n/;
var Xe = /* @__PURE__ */ (function(n) {
  return n[n.Simple = 0] = "Simple", n[n.TrackDel = 1] = "TrackDel", n[n.TrackBefore = 2] = "TrackBefore", n[n.TrackAfter = 3] = "TrackAfter", n;
})(Xe || (Xe = {}));
class Jt {
  // Sections are encoded as pairs of integers. The first is the
  // length in the current document, and the second is -1 for
  // unaffected sections, and the length of the replacement content
  // otherwise. So an insertion would be (0, n>0), a deletion (n>0,
  // 0), and a replacement two positive numbers.
  /**
  @internal
  */
  constructor(e) {
    this.sections = e;
  }
  /**
  The length of the document before the change.
  */
  get length() {
    let e = 0;
    for (let t = 0; t < this.sections.length; t += 2)
      e += this.sections[t];
    return e;
  }
  /**
  The length of the document after the change.
  */
  get newLength() {
    let e = 0;
    for (let t = 0; t < this.sections.length; t += 2) {
      let i = this.sections[t + 1];
      e += i < 0 ? this.sections[t] : i;
    }
    return e;
  }
  /**
  False when there are actual changes in this set.
  */
  get empty() {
    return this.sections.length == 0 || this.sections.length == 2 && this.sections[1] < 0;
  }
  /**
  Iterate over the unchanged parts left by these changes. `posA`
  provides the position of the range in the old document, `posB`
  the new position in the changed document.
  */
  iterGaps(e) {
    for (let t = 0, i = 0, r = 0; t < this.sections.length; ) {
      let s = this.sections[t++], o = this.sections[t++];
      o < 0 ? (e(i, r, s), r += s) : r += o, i += s;
    }
  }
  /**
  Iterate over the ranges changed by these changes. (See
  [`ChangeSet.iterChanges`](https://codemirror.net/6/docs/ref/#state.ChangeSet.iterChanges) for a
  variant that also provides you with the inserted text.)
  `fromA`/`toA` provides the extent of the change in the starting
  document, `fromB`/`toB` the extent of the replacement in the
  changed document.
  
  When `individual` is true, adjacent changes (which are kept
  separate for [position mapping](https://codemirror.net/6/docs/ref/#state.ChangeDesc.mapPos)) are
  reported separately.
  */
  iterChangedRanges(e, t = !1) {
    Ho(this, e, t);
  }
  /**
  Get a description of the inverted form of these changes.
  */
  get invertedDesc() {
    let e = [];
    for (let t = 0; t < this.sections.length; ) {
      let i = this.sections[t++], r = this.sections[t++];
      r < 0 ? e.push(i, r) : e.push(r, i);
    }
    return new Jt(e);
  }
  /**
  Compute the combined effect of applying another set of changes
  after this one. The length of the document after this set should
  match the length before `other`.
  */
  composeDesc(e) {
    return this.empty ? e : e.empty ? this : Pf(this, e);
  }
  /**
  Map this description, which should start with the same document
  as `other`, over another set of changes, so that it can be
  applied after it. When `before` is true, map as if the changes
  in `this` happened before the ones in `other`.
  */
  mapDesc(e, t = !1) {
    return e.empty ? this : zo(this, e, t);
  }
  mapPos(e, t = -1, i = Xe.Simple) {
    let r = 0, s = 0;
    for (let o = 0; o < this.sections.length; ) {
      let l = this.sections[o++], a = this.sections[o++], f = r + l;
      if (a < 0) {
        if (f > e)
          return s + (e - r);
        s += l;
      } else {
        if (i != Xe.Simple && f >= e && (i == Xe.TrackDel && r < e && f > e || i == Xe.TrackBefore && r < e || i == Xe.TrackAfter && f > e))
          return null;
        if (f > e || f == e && t < 0 && !l)
          return e == r || t < 0 ? s : s + a;
        s += a;
      }
      r = f;
    }
    if (e > r)
      throw new RangeError(`Position ${e} is out of range for changeset of length ${r}`);
    return s;
  }
  /**
  Check whether these changes touch a given range. When one of the
  changes entirely covers the range, the string `"cover"` is
  returned.
  */
  touchesRange(e, t = e) {
    for (let i = 0, r = 0; i < this.sections.length && r <= t; ) {
      let s = this.sections[i++], o = this.sections[i++], l = r + s;
      if (o >= 0 && r <= t && l >= e)
        return r < e && l > t ? "cover" : !0;
      r = l;
    }
    return !1;
  }
  /**
  @internal
  */
  toString() {
    let e = "";
    for (let t = 0; t < this.sections.length; ) {
      let i = this.sections[t++], r = this.sections[t++];
      e += (e ? " " : "") + i + (r >= 0 ? ":" + r : "");
    }
    return e;
  }
  /**
  Serialize this change desc to a JSON-representable value.
  */
  toJSON() {
    return this.sections;
  }
  /**
  Create a change desc from its JSON representation (as produced
  by [`toJSON`](https://codemirror.net/6/docs/ref/#state.ChangeDesc.toJSON).
  */
  static fromJSON(e) {
    if (!Array.isArray(e) || e.length % 2 || e.some((t) => typeof t != "number"))
      throw new RangeError("Invalid JSON representation of ChangeDesc");
    return new Jt(e);
  }
  /**
  @internal
  */
  static create(e) {
    return new Jt(e);
  }
}
class Fe extends Jt {
  constructor(e, t) {
    super(e), this.inserted = t;
  }
  /**
  Apply the changes to a document, returning the modified
  document.
  */
  apply(e) {
    if (this.length != e.length)
      throw new RangeError("Applying change set to a document with the wrong length");
    return Ho(this, (t, i, r, s, o) => e = e.replace(r, r + (i - t), o), !1), e;
  }
  mapDesc(e, t = !1) {
    return zo(this, e, t, !0);
  }
  /**
  Given the document as it existed _before_ the changes, return a
  change set that represents the inverse of this set, which could
  be used to go from the document created by the changes back to
  the document as it existed before the changes.
  */
  invert(e) {
    let t = this.sections.slice(), i = [];
    for (let r = 0, s = 0; r < t.length; r += 2) {
      let o = t[r], l = t[r + 1];
      if (l >= 0) {
        t[r] = l, t[r + 1] = o;
        let a = r >> 1;
        for (; i.length < a; )
          i.push(ge.empty);
        i.push(o ? e.slice(s, s + o) : ge.empty);
      }
      s += o;
    }
    return new Fe(t, i);
  }
  /**
  Combine two subsequent change sets into a single set. `other`
  must start in the document produced by `this`. If `this` goes
  `docA` → `docB` and `other` represents `docB` → `docC`, the
  returned value will represent the change `docA` → `docC`.
  */
  compose(e) {
    return this.empty ? e : e.empty ? this : Pf(this, e, !0);
  }
  /**
  Given another change set starting in the same document, maps this
  change set over the other, producing a new change set that can be
  applied to the document produced by applying `other`. When
  `before` is `true`, order changes as if `this` comes before
  `other`, otherwise (the default) treat `other` as coming first.
  
  Given two changes `A` and `B`, `A.compose(B.map(A))` and
  `B.compose(A.map(B, true))` will produce the same document. This
  provides a basic form of [operational
  transformation](https://en.wikipedia.org/wiki/Operational_transformation),
  and can be used for collaborative editing.
  */
  map(e, t = !1) {
    return e.empty ? this : zo(this, e, t, !0);
  }
  /**
  Iterate over the changed ranges in the document, calling `f` for
  each, with the range in the original document (`fromA`-`toA`)
  and the range that replaces it in the new document
  (`fromB`-`toB`).
  
  When `individual` is true, adjacent changes are reported
  separately.
  */
  iterChanges(e, t = !1) {
    Ho(this, e, t);
  }
  /**
  Get a [change description](https://codemirror.net/6/docs/ref/#state.ChangeDesc) for this change
  set.
  */
  get desc() {
    return Jt.create(this.sections);
  }
  /**
  @internal
  */
  filter(e) {
    let t = [], i = [], r = [], s = new Xn(this);
    e: for (let o = 0, l = 0; ; ) {
      let a = o == e.length ? 1e9 : e[o++];
      for (; l < a || l == a && s.len == 0; ) {
        if (s.done)
          break e;
        let u = Math.min(s.len, a - l);
        Je(r, u, -1);
        let g = s.ins == -1 ? -1 : s.off == 0 ? s.ins : 0;
        Je(t, u, g), g > 0 && vi(i, t, s.text), s.forward(u), l += u;
      }
      let f = e[o++];
      for (; l < f; ) {
        if (s.done)
          break e;
        let u = Math.min(s.len, f - l);
        Je(t, u, -1), Je(r, u, s.ins == -1 ? -1 : s.off == 0 ? s.ins : 0), s.forward(u), l += u;
      }
    }
    return {
      changes: new Fe(t, i),
      filtered: Jt.create(r)
    };
  }
  /**
  Serialize this change set to a JSON-representable value.
  */
  toJSON() {
    let e = [];
    for (let t = 0; t < this.sections.length; t += 2) {
      let i = this.sections[t], r = this.sections[t + 1];
      r < 0 ? e.push(i) : r == 0 ? e.push([i]) : e.push([i].concat(this.inserted[t >> 1].toJSON()));
    }
    return e;
  }
  /**
  Create a change set for the given changes, for a document of the
  given length, using `lineSep` as line separator.
  */
  static of(e, t, i) {
    let r = [], s = [], o = 0, l = null;
    function a(u = !1) {
      if (!u && !r.length)
        return;
      o < t && Je(r, t - o, -1);
      let g = new Fe(r, s);
      l = l ? l.compose(g.map(l)) : g, r = [], s = [], o = 0;
    }
    function f(u) {
      if (Array.isArray(u))
        for (let g of u)
          f(g);
      else if (u instanceof Fe) {
        if (u.length != t)
          throw new RangeError(`Mismatched change set length (got ${u.length}, expected ${t})`);
        a(), l = l ? l.compose(u.map(l)) : u;
      } else {
        let { from: g, to: y = g, insert: b } = u;
        if (g > y || g < 0 || y > t)
          throw new RangeError(`Invalid change range ${g} to ${y} (in doc of length ${t})`);
        let k = b ? typeof b == "string" ? ge.of(b.split(i || Vo)) : b : ge.empty, C = k.length;
        if (g == y && C == 0)
          return;
        g < o && a(), g > o && Je(r, g - o, -1), Je(r, y - g, C), vi(s, r, k), o = y;
      }
    }
    return f(e), a(!l), l;
  }
  /**
  Create an empty changeset of the given length.
  */
  static empty(e) {
    return new Fe(e ? [e, -1] : [], []);
  }
  /**
  Create a changeset from its JSON representation (as produced by
  [`toJSON`](https://codemirror.net/6/docs/ref/#state.ChangeSet.toJSON).
  */
  static fromJSON(e) {
    if (!Array.isArray(e))
      throw new RangeError("Invalid JSON representation of ChangeSet");
    let t = [], i = [];
    for (let r = 0; r < e.length; r++) {
      let s = e[r];
      if (typeof s == "number")
        t.push(s, -1);
      else {
        if (!Array.isArray(s) || typeof s[0] != "number" || s.some((o, l) => l && typeof o != "string"))
          throw new RangeError("Invalid JSON representation of ChangeSet");
        if (s.length == 1)
          t.push(s[0], 0);
        else {
          for (; i.length < r; )
            i.push(ge.empty);
          i[r] = ge.of(s.slice(1)), t.push(s[0], i[r].length);
        }
      }
    }
    return new Fe(t, i);
  }
  /**
  @internal
  */
  static createSet(e, t) {
    return new Fe(e, t);
  }
}
function Je(n, e, t, i = !1) {
  if (e == 0 && t <= 0)
    return;
  let r = n.length - 2;
  r >= 0 && t <= 0 && t == n[r + 1] ? n[r] += e : r >= 0 && e == 0 && n[r] == 0 ? n[r + 1] += t : i ? (n[r] += e, n[r + 1] += t) : n.push(e, t);
}
function vi(n, e, t) {
  if (t.length == 0)
    return;
  let i = e.length - 2 >> 1;
  if (i < n.length)
    n[n.length - 1] = n[n.length - 1].append(t);
  else {
    for (; n.length < i; )
      n.push(ge.empty);
    n.push(t);
  }
}
function Ho(n, e, t) {
  let i = n.inserted;
  for (let r = 0, s = 0, o = 0; o < n.sections.length; ) {
    let l = n.sections[o++], a = n.sections[o++];
    if (a < 0)
      r += l, s += l;
    else {
      let f = r, u = s, g = ge.empty;
      for (; f += l, u += a, a && i && (g = g.append(i[o - 2 >> 1])), !(t || o == n.sections.length || n.sections[o + 1] < 0); )
        l = n.sections[o++], a = n.sections[o++];
      e(r, f, s, u, g), r = f, s = u;
    }
  }
}
function zo(n, e, t, i = !1) {
  let r = [], s = i ? [] : null, o = new Xn(n), l = new Xn(e);
  for (let a = -1; ; ) {
    if (o.done && l.len || l.done && o.len)
      throw new Error("Mismatched change set lengths");
    if (o.ins == -1 && l.ins == -1) {
      let f = Math.min(o.len, l.len);
      Je(r, f, -1), o.forward(f), l.forward(f);
    } else if (l.ins >= 0 && (o.ins < 0 || a == o.i || o.off == 0 && (l.len < o.len || l.len == o.len && !t))) {
      let f = l.len;
      for (Je(r, l.ins, -1); f; ) {
        let u = Math.min(o.len, f);
        o.ins >= 0 && a < o.i && o.len <= u && (Je(r, 0, o.ins), s && vi(s, r, o.text), a = o.i), o.forward(u), f -= u;
      }
      l.next();
    } else if (o.ins >= 0) {
      let f = 0, u = o.len;
      for (; u; )
        if (l.ins == -1) {
          let g = Math.min(u, l.len);
          f += g, u -= g, l.forward(g);
        } else if (l.ins == 0 && l.len < u)
          u -= l.len, l.next();
        else
          break;
      Je(r, f, a < o.i ? o.ins : 0), s && a < o.i && vi(s, r, o.text), a = o.i, o.forward(o.len - u);
    } else {
      if (o.done && l.done)
        return s ? Fe.createSet(r, s) : Jt.create(r);
      throw new Error("Mismatched change set lengths");
    }
  }
}
function Pf(n, e, t = !1) {
  let i = [], r = t ? [] : null, s = new Xn(n), o = new Xn(e);
  for (let l = !1; ; ) {
    if (s.done && o.done)
      return r ? Fe.createSet(i, r) : Jt.create(i);
    if (s.ins == 0)
      Je(i, s.len, 0, l), s.next();
    else if (o.len == 0 && !o.done)
      Je(i, 0, o.ins, l), r && vi(r, i, o.text), o.next();
    else {
      if (s.done || o.done)
        throw new Error("Mismatched change set lengths");
      {
        let a = Math.min(s.len2, o.len), f = i.length;
        if (s.ins == -1) {
          let u = o.ins == -1 ? -1 : o.off ? 0 : o.ins;
          Je(i, a, u, l), r && u && vi(r, i, o.text);
        } else o.ins == -1 ? (Je(i, s.off ? 0 : s.len, a, l), r && vi(r, i, s.textBit(a))) : (Je(i, s.off ? 0 : s.len, o.off ? 0 : o.ins, l), r && !o.off && vi(r, i, o.text));
        l = (s.ins > a || o.ins >= 0 && o.len > a) && (l || i.length > f), s.forward2(a), o.forward(a);
      }
    }
  }
}
class Xn {
  constructor(e) {
    this.set = e, this.i = 0, this.next();
  }
  next() {
    let { sections: e } = this.set;
    this.i < e.length ? (this.len = e[this.i++], this.ins = e[this.i++]) : (this.len = 0, this.ins = -2), this.off = 0;
  }
  get done() {
    return this.ins == -2;
  }
  get len2() {
    return this.ins < 0 ? this.len : this.ins;
  }
  get text() {
    let { inserted: e } = this.set, t = this.i - 2 >> 1;
    return t >= e.length ? ge.empty : e[t];
  }
  textBit(e) {
    let { inserted: t } = this.set, i = this.i - 2 >> 1;
    return i >= t.length && !e ? ge.empty : t[i].slice(this.off, e == null ? void 0 : this.off + e);
  }
  forward(e) {
    e == this.len ? this.next() : (this.len -= e, this.off += e);
  }
  forward2(e) {
    this.ins == -1 ? this.forward(e) : e == this.ins ? this.next() : (this.ins -= e, this.off += e);
  }
}
class Vi {
  constructor(e, t, i) {
    this.from = e, this.to = t, this.flags = i;
  }
  /**
  The anchor of the range—the side that doesn't move when you
  extend it.
  */
  get anchor() {
    return this.flags & 32 ? this.to : this.from;
  }
  /**
  The head of the range, which is moved when the range is
  [extended](https://codemirror.net/6/docs/ref/#state.SelectionRange.extend).
  */
  get head() {
    return this.flags & 32 ? this.from : this.to;
  }
  /**
  True when `anchor` and `head` are at the same position.
  */
  get empty() {
    return this.from == this.to;
  }
  /**
  If this is a cursor that is explicitly associated with the
  character on one of its sides, this returns the side. -1 means
  the character before its position, 1 the character after, and 0
  means no association.
  */
  get assoc() {
    return this.flags & 8 ? -1 : this.flags & 16 ? 1 : 0;
  }
  /**
  The bidirectional text level associated with this cursor, if
  any.
  */
  get bidiLevel() {
    let e = this.flags & 7;
    return e == 7 ? null : e;
  }
  /**
  The goal column (stored vertical offset) associated with a
  cursor. This is used to preserve the vertical position when
  [moving](https://codemirror.net/6/docs/ref/#view.EditorView.moveVertically) across
  lines of different length.
  */
  get goalColumn() {
    let e = this.flags >> 6;
    return e == 16777215 ? void 0 : e;
  }
  /**
  Map this range through a change, producing a valid range in the
  updated document.
  */
  map(e, t = -1) {
    let i, r;
    return this.empty ? i = r = e.mapPos(this.from, t) : (i = e.mapPos(this.from, 1), r = e.mapPos(this.to, -1)), i == this.from && r == this.to ? this : new Vi(i, r, this.flags);
  }
  /**
  Extend this range to cover at least `from` to `to`.
  */
  extend(e, t = e) {
    if (e <= this.anchor && t >= this.anchor)
      return E.range(e, t);
    let i = Math.abs(e - this.anchor) > Math.abs(t - this.anchor) ? e : t;
    return E.range(this.anchor, i);
  }
  /**
  Compare this range to another range.
  */
  eq(e, t = !1) {
    return this.anchor == e.anchor && this.head == e.head && this.goalColumn == e.goalColumn && (!t || !this.empty || this.assoc == e.assoc);
  }
  /**
  Return a JSON-serializable object representing the range.
  */
  toJSON() {
    return { anchor: this.anchor, head: this.head };
  }
  /**
  Convert a JSON representation of a range to a `SelectionRange`
  instance.
  */
  static fromJSON(e) {
    if (!e || typeof e.anchor != "number" || typeof e.head != "number")
      throw new RangeError("Invalid JSON representation for SelectionRange");
    return E.range(e.anchor, e.head);
  }
  /**
  @internal
  */
  static create(e, t, i) {
    return new Vi(e, t, i);
  }
}
class E {
  constructor(e, t) {
    this.ranges = e, this.mainIndex = t;
  }
  /**
  Map a selection through a change. Used to adjust the selection
  position for changes.
  */
  map(e, t = -1) {
    return e.empty ? this : E.create(this.ranges.map((i) => i.map(e, t)), this.mainIndex);
  }
  /**
  Compare this selection to another selection. By default, ranges
  are compared only by position. When `includeAssoc` is true,
  cursor ranges must also have the same
  [`assoc`](https://codemirror.net/6/docs/ref/#state.SelectionRange.assoc) value.
  */
  eq(e, t = !1) {
    if (this.ranges.length != e.ranges.length || this.mainIndex != e.mainIndex)
      return !1;
    for (let i = 0; i < this.ranges.length; i++)
      if (!this.ranges[i].eq(e.ranges[i], t))
        return !1;
    return !0;
  }
  /**
  Get the primary selection range. Usually, you should make sure
  your code applies to _all_ ranges, by using methods like
  [`changeByRange`](https://codemirror.net/6/docs/ref/#state.EditorState.changeByRange).
  */
  get main() {
    return this.ranges[this.mainIndex];
  }
  /**
  Make sure the selection only has one range. Returns a selection
  holding only the main range from this selection.
  */
  asSingle() {
    return this.ranges.length == 1 ? this : new E([this.main], 0);
  }
  /**
  Extend this selection with an extra range.
  */
  addRange(e, t = !0) {
    return E.create([e].concat(this.ranges), t ? 0 : this.mainIndex + 1);
  }
  /**
  Replace a given range with another range, and then normalize the
  selection to merge and sort ranges if necessary.
  */
  replaceRange(e, t = this.mainIndex) {
    let i = this.ranges.slice();
    return i[t] = e, E.create(i, this.mainIndex);
  }
  /**
  Convert this selection to an object that can be serialized to
  JSON.
  */
  toJSON() {
    return { ranges: this.ranges.map((e) => e.toJSON()), main: this.mainIndex };
  }
  /**
  Create a selection from a JSON representation.
  */
  static fromJSON(e) {
    if (!e || !Array.isArray(e.ranges) || typeof e.main != "number" || e.main >= e.ranges.length)
      throw new RangeError("Invalid JSON representation for EditorSelection");
    return new E(e.ranges.map((t) => Vi.fromJSON(t)), e.main);
  }
  /**
  Create a selection holding a single range.
  */
  static single(e, t = e) {
    return new E([E.range(e, t)], 0);
  }
  /**
  Sort and merge the given set of ranges, creating a valid
  selection.
  */
  static create(e, t = 0) {
    if (e.length == 0)
      throw new RangeError("A selection needs at least one range");
    for (let i = 0, r = 0; r < e.length; r++) {
      let s = e[r];
      if (s.empty ? s.from <= i : s.from < i)
        return E.normalized(e.slice(), t);
      i = s.to;
    }
    return new E(e, t);
  }
  /**
  Create a cursor selection range at the given position. You can
  safely ignore the optional arguments in most situations.
  */
  static cursor(e, t = 0, i, r) {
    return Vi.create(e, e, (t == 0 ? 0 : t < 0 ? 8 : 16) | (i == null ? 7 : Math.min(6, i)) | (r ?? 16777215) << 6);
  }
  /**
  Create a selection range.
  */
  static range(e, t, i, r) {
    let s = (i ?? 16777215) << 6 | (r == null ? 7 : Math.min(6, r));
    return t < e ? Vi.create(t, e, 48 | s) : Vi.create(e, t, (t > e ? 8 : 0) | s);
  }
  /**
  @internal
  */
  static normalized(e, t = 0) {
    let i = e[t];
    e.sort((r, s) => r.from - s.from), t = e.indexOf(i);
    for (let r = 1; r < e.length; r++) {
      let s = e[r], o = e[r - 1];
      if (s.empty ? s.from <= o.to : s.from < o.to) {
        let l = o.from, a = Math.max(s.to, o.to);
        r <= t && t--, e.splice(--r, 2, s.anchor > s.head ? E.range(a, l) : E.range(l, a));
      }
    }
    return new E(e, t);
  }
}
function Rf(n, e) {
  for (let t of n.ranges)
    if (t.to > e)
      throw new RangeError("Selection points outside of document");
}
let Vl = 0;
class U {
  constructor(e, t, i, r, s) {
    this.combine = e, this.compareInput = t, this.compare = i, this.isStatic = r, this.id = Vl++, this.default = e([]), this.extensions = typeof s == "function" ? s(this) : s;
  }
  /**
  Returns a facet reader for this facet, which can be used to
  [read](https://codemirror.net/6/docs/ref/#state.EditorState.facet) it but not to define values for it.
  */
  get reader() {
    return this;
  }
  /**
  Define a new facet.
  */
  static define(e = {}) {
    return new U(e.combine || ((t) => t), e.compareInput || ((t, i) => t === i), e.compare || (e.combine ? (t, i) => t === i : Hl), !!e.static, e.enables);
  }
  /**
  Returns an extension that adds the given value to this facet.
  */
  of(e) {
    return new _r([], this, 0, e);
  }
  /**
  Create an extension that computes a value for the facet from a
  state. You must take care to declare the parts of the state that
  this value depends on, since your function is only called again
  for a new state when one of those parts changed.
  
  In cases where your value depends only on a single field, you'll
  want to use the [`from`](https://codemirror.net/6/docs/ref/#state.Facet.from) method instead.
  */
  compute(e, t) {
    if (this.isStatic)
      throw new Error("Can't compute a static facet");
    return new _r(e, this, 1, t);
  }
  /**
  Create an extension that computes zero or more values for this
  facet from a state.
  */
  computeN(e, t) {
    if (this.isStatic)
      throw new Error("Can't compute a static facet");
    return new _r(e, this, 2, t);
  }
  from(e, t) {
    return t || (t = (i) => i), this.compute([e], (i) => t(i.field(e)));
  }
}
function Hl(n, e) {
  return n == e || n.length == e.length && n.every((t, i) => t === e[i]);
}
class _r {
  constructor(e, t, i, r) {
    this.dependencies = e, this.facet = t, this.type = i, this.value = r, this.id = Vl++;
  }
  dynamicSlot(e) {
    var t;
    let i = this.value, r = this.facet.compareInput, s = this.id, o = e[s] >> 1, l = this.type == 2, a = !1, f = !1, u = [];
    for (let g of this.dependencies)
      g == "doc" ? a = !0 : g == "selection" ? f = !0 : (((t = e[g.id]) !== null && t !== void 0 ? t : 1) & 1) == 0 && u.push(e[g.id]);
    return {
      create(g) {
        return g.values[o] = i(g), 1;
      },
      update(g, y) {
        if (a && y.docChanged || f && (y.docChanged || y.selection) || $o(g, u)) {
          let b = i(g);
          if (l ? !ph(b, g.values[o], r) : !r(b, g.values[o]))
            return g.values[o] = b, 1;
        }
        return 0;
      },
      reconfigure: (g, y) => {
        let b, k = y.config.address[s];
        if (k != null) {
          let C = is(y, k);
          if (this.dependencies.every((M) => M instanceof U ? y.facet(M) === g.facet(M) : M instanceof $e ? y.field(M, !1) == g.field(M, !1) : !0) || (l ? ph(b = i(g), C, r) : r(b = i(g), C)))
            return g.values[o] = C, 0;
        } else
          b = i(g);
        return g.values[o] = b, 1;
      }
    };
  }
}
function ph(n, e, t) {
  if (n.length != e.length)
    return !1;
  for (let i = 0; i < n.length; i++)
    if (!t(n[i], e[i]))
      return !1;
  return !0;
}
function $o(n, e) {
  let t = !1;
  for (let i of e)
    qn(n, i) & 1 && (t = !0);
  return t;
}
function Ng(n, e, t) {
  let i = t.map((a) => n[a.id]), r = t.map((a) => a.type), s = i.filter((a) => !(a & 1)), o = n[e.id] >> 1;
  function l(a) {
    let f = [];
    for (let u = 0; u < i.length; u++) {
      let g = is(a, i[u]);
      if (r[u] == 2)
        for (let y of g)
          f.push(y);
      else
        f.push(g);
    }
    return e.combine(f);
  }
  return {
    create(a) {
      for (let f of i)
        qn(a, f);
      return a.values[o] = l(a), 1;
    },
    update(a, f) {
      if (!$o(a, s))
        return 0;
      let u = l(a);
      return e.compare(u, a.values[o]) ? 0 : (a.values[o] = u, 1);
    },
    reconfigure(a, f) {
      let u = $o(a, i), g = f.config.facets[e.id], y = f.facet(e);
      if (g && !u && Hl(t, g))
        return a.values[o] = y, 0;
      let b = l(a);
      return e.compare(b, y) ? (a.values[o] = y, 0) : (a.values[o] = b, 1);
    }
  };
}
const kr = /* @__PURE__ */ U.define({ static: !0 });
class $e {
  constructor(e, t, i, r, s) {
    this.id = e, this.createF = t, this.updateF = i, this.compareF = r, this.spec = s, this.provides = void 0;
  }
  /**
  Define a state field.
  */
  static define(e) {
    let t = new $e(Vl++, e.create, e.update, e.compare || ((i, r) => i === r), e);
    return e.provide && (t.provides = e.provide(t)), t;
  }
  create(e) {
    let t = e.facet(kr).find((i) => i.field == this);
    return (t?.create || this.createF)(e);
  }
  /**
  @internal
  */
  slot(e) {
    let t = e[this.id] >> 1;
    return {
      create: (i) => (i.values[t] = this.create(i), 1),
      update: (i, r) => {
        let s = i.values[t], o = this.updateF(s, r);
        return this.compareF(s, o) ? 0 : (i.values[t] = o, 1);
      },
      reconfigure: (i, r) => {
        let s = i.facet(kr), o = r.facet(kr), l;
        return (l = s.find((a) => a.field == this)) && l != o.find((a) => a.field == this) ? (i.values[t] = l.create(i), 1) : r.config.address[this.id] != null ? (i.values[t] = r.field(this), 0) : (i.values[t] = this.create(i), 1);
      }
    };
  }
  /**
  Returns an extension that enables this field and overrides the
  way it is initialized. Can be useful when you need to provide a
  non-default starting value for the field.
  */
  init(e) {
    return [this, kr.of({ field: this, create: e })];
  }
  /**
  State field instances can be used as
  [`Extension`](https://codemirror.net/6/docs/ref/#state.Extension) values to enable the field in a
  given state.
  */
  get extension() {
    return this;
  }
}
const Wi = { lowest: 4, low: 3, default: 2, high: 1, highest: 0 };
function Bn(n) {
  return (e) => new Lf(e, n);
}
const Ti = {
  /**
  The highest precedence level, for extensions that should end up
  near the start of the precedence ordering.
  */
  highest: /* @__PURE__ */ Bn(Wi.highest),
  /**
  A higher-than-default precedence, for extensions that should
  come before those with default precedence.
  */
  high: /* @__PURE__ */ Bn(Wi.high),
  /**
  The default precedence, which is also used for extensions
  without an explicit precedence.
  */
  default: /* @__PURE__ */ Bn(Wi.default),
  /**
  A lower-than-default precedence.
  */
  low: /* @__PURE__ */ Bn(Wi.low),
  /**
  The lowest precedence level. Meant for things that should end up
  near the end of the extension order.
  */
  lowest: /* @__PURE__ */ Bn(Wi.lowest)
};
class Lf {
  constructor(e, t) {
    this.inner = e, this.prec = t;
  }
}
class Ds {
  /**
  Create an instance of this compartment to add to your [state
  configuration](https://codemirror.net/6/docs/ref/#state.EditorStateConfig.extensions).
  */
  of(e) {
    return new qo(this, e);
  }
  /**
  Create an [effect](https://codemirror.net/6/docs/ref/#state.TransactionSpec.effects) that
  reconfigures this compartment.
  */
  reconfigure(e) {
    return Ds.reconfigure.of({ compartment: this, extension: e });
  }
  /**
  Get the current content of the compartment in the state, or
  `undefined` if it isn't present.
  */
  get(e) {
    return e.config.compartments.get(this);
  }
}
class qo {
  constructor(e, t) {
    this.compartment = e, this.inner = t;
  }
}
class ts {
  constructor(e, t, i, r, s, o) {
    for (this.base = e, this.compartments = t, this.dynamicSlots = i, this.address = r, this.staticValues = s, this.facets = o, this.statusTemplate = []; this.statusTemplate.length < i.length; )
      this.statusTemplate.push(
        0
        /* SlotStatus.Unresolved */
      );
  }
  staticFacet(e) {
    let t = this.address[e.id];
    return t == null ? e.default : this.staticValues[t >> 1];
  }
  static resolve(e, t, i) {
    let r = [], s = /* @__PURE__ */ Object.create(null), o = /* @__PURE__ */ new Map();
    for (let y of Fg(e, t, o))
      y instanceof $e ? r.push(y) : (s[y.facet.id] || (s[y.facet.id] = [])).push(y);
    let l = /* @__PURE__ */ Object.create(null), a = [], f = [];
    for (let y of r)
      l[y.id] = f.length << 1, f.push((b) => y.slot(b));
    let u = i?.config.facets;
    for (let y in s) {
      let b = s[y], k = b[0].facet, C = u && u[y] || [];
      if (b.every(
        (M) => M.type == 0
        /* Provider.Static */
      ))
        if (l[k.id] = a.length << 1 | 1, Hl(C, b))
          a.push(i.facet(k));
        else {
          let M = k.combine(b.map((R) => R.value));
          a.push(i && k.compare(M, i.facet(k)) ? i.facet(k) : M);
        }
      else {
        for (let M of b)
          M.type == 0 ? (l[M.id] = a.length << 1 | 1, a.push(M.value)) : (l[M.id] = f.length << 1, f.push((R) => M.dynamicSlot(R)));
        l[k.id] = f.length << 1, f.push((M) => Ng(M, k, b));
      }
    }
    let g = f.map((y) => y(l));
    return new ts(e, o, g, l, a, s);
  }
}
function Fg(n, e, t) {
  let i = [[], [], [], [], []], r = /* @__PURE__ */ new Map();
  function s(o, l) {
    let a = r.get(o);
    if (a != null) {
      if (a <= l)
        return;
      let f = i[a].indexOf(o);
      f > -1 && i[a].splice(f, 1), o instanceof qo && t.delete(o.compartment);
    }
    if (r.set(o, l), Array.isArray(o))
      for (let f of o)
        s(f, l);
    else if (o instanceof qo) {
      if (t.has(o.compartment))
        throw new RangeError("Duplicate use of compartment in extensions");
      let f = e.get(o.compartment) || o.inner;
      t.set(o.compartment, f), s(f, l);
    } else if (o instanceof Lf)
      s(o.inner, o.prec);
    else if (o instanceof $e)
      i[l].push(o), o.provides && s(o.provides, l);
    else if (o instanceof _r)
      i[l].push(o), o.facet.extensions && s(o.facet.extensions, Wi.default);
    else {
      let f = o.extension;
      if (!f)
        throw new Error(`Unrecognized extension value in extension set (${o}). This sometimes happens because multiple instances of @codemirror/state are loaded, breaking instanceof checks.`);
      s(f, l);
    }
  }
  return s(n, Wi.default), i.reduce((o, l) => o.concat(l));
}
function qn(n, e) {
  if (e & 1)
    return 2;
  let t = e >> 1, i = n.status[t];
  if (i == 4)
    throw new Error("Cyclic dependency between fields and/or facets");
  if (i & 2)
    return i;
  n.status[t] = 4;
  let r = n.computeSlot(n, n.config.dynamicSlots[t]);
  return n.status[t] = 2 | r;
}
function is(n, e) {
  return e & 1 ? n.config.staticValues[e >> 1] : n.values[e >> 1];
}
const Df = /* @__PURE__ */ U.define(), Ko = /* @__PURE__ */ U.define({
  combine: (n) => n.some((e) => e),
  static: !0
}), Bf = /* @__PURE__ */ U.define({
  combine: (n) => n.length ? n[0] : void 0,
  static: !0
}), Ef = /* @__PURE__ */ U.define(), If = /* @__PURE__ */ U.define(), Nf = /* @__PURE__ */ U.define(), Ff = /* @__PURE__ */ U.define({
  combine: (n) => n.length ? n[0] : !1
});
class hi {
  /**
  @internal
  */
  constructor(e, t) {
    this.type = e, this.value = t;
  }
  /**
  Define a new type of annotation.
  */
  static define() {
    return new Wg();
  }
}
class Wg {
  /**
  Create an instance of this annotation.
  */
  of(e) {
    return new hi(this, e);
  }
}
class Qg {
  /**
  @internal
  */
  constructor(e) {
    this.map = e;
  }
  /**
  Create a [state effect](https://codemirror.net/6/docs/ref/#state.StateEffect) instance of this
  type.
  */
  of(e) {
    return new ne(this, e);
  }
}
class ne {
  /**
  @internal
  */
  constructor(e, t) {
    this.type = e, this.value = t;
  }
  /**
  Map this effect through a position mapping. Will return
  `undefined` when that ends up deleting the effect.
  */
  map(e) {
    let t = this.type.map(this.value, e);
    return t === void 0 ? void 0 : t == this.value ? this : new ne(this.type, t);
  }
  /**
  Tells you whether this effect object is of a given
  [type](https://codemirror.net/6/docs/ref/#state.StateEffectType).
  */
  is(e) {
    return this.type == e;
  }
  /**
  Define a new effect type. The type parameter indicates the type
  of values that his effect holds. It should be a type that
  doesn't include `undefined`, since that is used in
  [mapping](https://codemirror.net/6/docs/ref/#state.StateEffect.map) to indicate that an effect is
  removed.
  */
  static define(e = {}) {
    return new Qg(e.map || ((t) => t));
  }
  /**
  Map an array of effects through a change set.
  */
  static mapEffects(e, t) {
    if (!e.length)
      return e;
    let i = [];
    for (let r of e) {
      let s = r.map(t);
      s && i.push(s);
    }
    return i;
  }
}
ne.reconfigure = /* @__PURE__ */ ne.define();
ne.appendConfig = /* @__PURE__ */ ne.define();
class Qe {
  constructor(e, t, i, r, s, o) {
    this.startState = e, this.changes = t, this.selection = i, this.effects = r, this.annotations = s, this.scrollIntoView = o, this._doc = null, this._state = null, i && Rf(i, t.newLength), s.some((l) => l.type == Qe.time) || (this.annotations = s.concat(Qe.time.of(Date.now())));
  }
  /**
  @internal
  */
  static create(e, t, i, r, s, o) {
    return new Qe(e, t, i, r, s, o);
  }
  /**
  The new document produced by the transaction. Contrary to
  [`.state`](https://codemirror.net/6/docs/ref/#state.Transaction.state)`.doc`, accessing this won't
  force the entire new state to be computed right away, so it is
  recommended that [transaction
  filters](https://codemirror.net/6/docs/ref/#state.EditorState^transactionFilter) use this getter
  when they need to look at the new document.
  */
  get newDoc() {
    return this._doc || (this._doc = this.changes.apply(this.startState.doc));
  }
  /**
  The new selection produced by the transaction. If
  [`this.selection`](https://codemirror.net/6/docs/ref/#state.Transaction.selection) is undefined,
  this will [map](https://codemirror.net/6/docs/ref/#state.EditorSelection.map) the start state's
  current selection through the changes made by the transaction.
  */
  get newSelection() {
    return this.selection || this.startState.selection.map(this.changes);
  }
  /**
  The new state created by the transaction. Computed on demand
  (but retained for subsequent access), so it is recommended not to
  access it in [transaction
  filters](https://codemirror.net/6/docs/ref/#state.EditorState^transactionFilter) when possible.
  */
  get state() {
    return this._state || this.startState.applyTransaction(this), this._state;
  }
  /**
  Get the value of the given annotation type, if any.
  */
  annotation(e) {
    for (let t of this.annotations)
      if (t.type == e)
        return t.value;
  }
  /**
  Indicates whether the transaction changed the document.
  */
  get docChanged() {
    return !this.changes.empty;
  }
  /**
  Indicates whether this transaction reconfigures the state
  (through a [configuration compartment](https://codemirror.net/6/docs/ref/#state.Compartment) or
  with a top-level configuration
  [effect](https://codemirror.net/6/docs/ref/#state.StateEffect^reconfigure).
  */
  get reconfigured() {
    return this.startState.config != this.state.config;
  }
  /**
  Returns true if the transaction has a [user
  event](https://codemirror.net/6/docs/ref/#state.Transaction^userEvent) annotation that is equal to
  or more specific than `event`. For example, if the transaction
  has `"select.pointer"` as user event, `"select"` and
  `"select.pointer"` will match it.
  */
  isUserEvent(e) {
    let t = this.annotation(Qe.userEvent);
    return !!(t && (t == e || t.length > e.length && t.slice(0, e.length) == e && t[e.length] == "."));
  }
}
Qe.time = /* @__PURE__ */ hi.define();
Qe.userEvent = /* @__PURE__ */ hi.define();
Qe.addToHistory = /* @__PURE__ */ hi.define();
Qe.remote = /* @__PURE__ */ hi.define();
function Vg(n, e) {
  let t = [];
  for (let i = 0, r = 0; ; ) {
    let s, o;
    if (i < n.length && (r == e.length || e[r] >= n[i]))
      s = n[i++], o = n[i++];
    else if (r < e.length)
      s = e[r++], o = e[r++];
    else
      return t;
    !t.length || t[t.length - 1] < s ? t.push(s, o) : t[t.length - 1] < o && (t[t.length - 1] = o);
  }
}
function Wf(n, e, t) {
  var i;
  let r, s, o;
  return t ? (r = e.changes, s = Fe.empty(e.changes.length), o = n.changes.compose(e.changes)) : (r = e.changes.map(n.changes), s = n.changes.mapDesc(e.changes, !0), o = n.changes.compose(r)), {
    changes: o,
    selection: e.selection ? e.selection.map(s) : (i = n.selection) === null || i === void 0 ? void 0 : i.map(r),
    effects: ne.mapEffects(n.effects, r).concat(ne.mapEffects(e.effects, s)),
    annotations: n.annotations.length ? n.annotations.concat(e.annotations) : e.annotations,
    scrollIntoView: n.scrollIntoView || e.scrollIntoView
  };
}
function _o(n, e, t) {
  let i = e.selection, r = hn(e.annotations);
  return e.userEvent && (r = r.concat(Qe.userEvent.of(e.userEvent))), {
    changes: e.changes instanceof Fe ? e.changes : Fe.of(e.changes || [], t, n.facet(Bf)),
    selection: i && (i instanceof E ? i : E.single(i.anchor, i.head)),
    effects: hn(e.effects),
    annotations: r,
    scrollIntoView: !!e.scrollIntoView
  };
}
function Qf(n, e, t) {
  let i = _o(n, e.length ? e[0] : {}, n.doc.length);
  e.length && e[0].filter === !1 && (t = !1);
  for (let s = 1; s < e.length; s++) {
    e[s].filter === !1 && (t = !1);
    let o = !!e[s].sequential;
    i = Wf(i, _o(n, e[s], o ? i.changes.newLength : n.doc.length), o);
  }
  let r = Qe.create(n, i.changes, i.selection, i.effects, i.annotations, i.scrollIntoView);
  return zg(t ? Hg(r) : r);
}
function Hg(n) {
  let e = n.startState, t = !0;
  for (let r of e.facet(Ef)) {
    let s = r(n);
    if (s === !1) {
      t = !1;
      break;
    }
    Array.isArray(s) && (t = t === !0 ? s : Vg(t, s));
  }
  if (t !== !0) {
    let r, s;
    if (t === !1)
      s = n.changes.invertedDesc, r = Fe.empty(e.doc.length);
    else {
      let o = n.changes.filter(t);
      r = o.changes, s = o.filtered.mapDesc(o.changes).invertedDesc;
    }
    n = Qe.create(e, r, n.selection && n.selection.map(s), ne.mapEffects(n.effects, s), n.annotations, n.scrollIntoView);
  }
  let i = e.facet(If);
  for (let r = i.length - 1; r >= 0; r--) {
    let s = i[r](n);
    s instanceof Qe ? n = s : Array.isArray(s) && s.length == 1 && s[0] instanceof Qe ? n = s[0] : n = Qf(e, hn(s), !1);
  }
  return n;
}
function zg(n) {
  let e = n.startState, t = e.facet(Nf), i = n;
  for (let r = t.length - 1; r >= 0; r--) {
    let s = t[r](n);
    s && Object.keys(s).length && (i = Wf(i, _o(e, s, n.changes.newLength), !0));
  }
  return i == n ? n : Qe.create(e, n.changes, n.selection, i.effects, i.annotations, i.scrollIntoView);
}
const $g = [];
function hn(n) {
  return n == null ? $g : Array.isArray(n) ? n : [n];
}
var Me = /* @__PURE__ */ (function(n) {
  return n[n.Word = 0] = "Word", n[n.Space = 1] = "Space", n[n.Other = 2] = "Other", n;
})(Me || (Me = {}));
const qg = /[\u00df\u0587\u0590-\u05f4\u0600-\u06ff\u3040-\u309f\u30a0-\u30ff\u3400-\u4db5\u4e00-\u9fcc\uac00-\ud7af]/;
let jo;
try {
  jo = /* @__PURE__ */ new RegExp("[\\p{Alphabetic}\\p{Number}_]", "u");
} catch {
}
function Kg(n) {
  if (jo)
    return jo.test(n);
  for (let e = 0; e < n.length; e++) {
    let t = n[e];
    if (/\w/.test(t) || t > "" && (t.toUpperCase() != t.toLowerCase() || qg.test(t)))
      return !0;
  }
  return !1;
}
function _g(n) {
  return (e) => {
    if (!/\S/.test(e))
      return Me.Space;
    if (Kg(e))
      return Me.Word;
    for (let t = 0; t < n.length; t++)
      if (e.indexOf(n[t]) > -1)
        return Me.Word;
    return Me.Other;
  };
}
class pe {
  constructor(e, t, i, r, s, o) {
    this.config = e, this.doc = t, this.selection = i, this.values = r, this.status = e.statusTemplate.slice(), this.computeSlot = s, o && (o._state = this);
    for (let l = 0; l < this.config.dynamicSlots.length; l++)
      qn(this, l << 1);
    this.computeSlot = null;
  }
  field(e, t = !0) {
    let i = this.config.address[e.id];
    if (i == null) {
      if (t)
        throw new RangeError("Field is not present in this state");
      return;
    }
    return qn(this, i), is(this, i);
  }
  /**
  Create a [transaction](https://codemirror.net/6/docs/ref/#state.Transaction) that updates this
  state. Any number of [transaction specs](https://codemirror.net/6/docs/ref/#state.TransactionSpec)
  can be passed. Unless
  [`sequential`](https://codemirror.net/6/docs/ref/#state.TransactionSpec.sequential) is set, the
  [changes](https://codemirror.net/6/docs/ref/#state.TransactionSpec.changes) (if any) of each spec
  are assumed to start in the _current_ document (not the document
  produced by previous specs), and its
  [selection](https://codemirror.net/6/docs/ref/#state.TransactionSpec.selection) and
  [effects](https://codemirror.net/6/docs/ref/#state.TransactionSpec.effects) are assumed to refer
  to the document created by its _own_ changes. The resulting
  transaction contains the combined effect of all the different
  specs. For [selection](https://codemirror.net/6/docs/ref/#state.TransactionSpec.selection), later
  specs take precedence over earlier ones.
  */
  update(...e) {
    return Qf(this, e, !0);
  }
  /**
  @internal
  */
  applyTransaction(e) {
    let t = this.config, { base: i, compartments: r } = t;
    for (let l of e.effects)
      l.is(Ds.reconfigure) ? (t && (r = /* @__PURE__ */ new Map(), t.compartments.forEach((a, f) => r.set(f, a)), t = null), r.set(l.value.compartment, l.value.extension)) : l.is(ne.reconfigure) ? (t = null, i = l.value) : l.is(ne.appendConfig) && (t = null, i = hn(i).concat(l.value));
    let s;
    t ? s = e.startState.values.slice() : (t = ts.resolve(i, r, this), s = new pe(t, this.doc, this.selection, t.dynamicSlots.map(() => null), (a, f) => f.reconfigure(a, this), null).values);
    let o = e.startState.facet(Ko) ? e.newSelection : e.newSelection.asSingle();
    new pe(t, e.newDoc, o, s, (l, a) => a.update(l, e), e);
  }
  /**
  Create a [transaction spec](https://codemirror.net/6/docs/ref/#state.TransactionSpec) that
  replaces every selection range with the given content.
  */
  replaceSelection(e) {
    return typeof e == "string" && (e = this.toText(e)), this.changeByRange((t) => ({
      changes: { from: t.from, to: t.to, insert: e },
      range: E.cursor(t.from + e.length)
    }));
  }
  /**
  Create a set of changes and a new selection by running the given
  function for each range in the active selection. The function
  can return an optional set of changes (in the coordinate space
  of the start document), plus an updated range (in the coordinate
  space of the document produced by the call's own changes). This
  method will merge all the changes and ranges into a single
  changeset and selection, and return it as a [transaction
  spec](https://codemirror.net/6/docs/ref/#state.TransactionSpec), which can be passed to
  [`update`](https://codemirror.net/6/docs/ref/#state.EditorState.update).
  */
  changeByRange(e) {
    let t = this.selection, i = e(t.ranges[0]), r = this.changes(i.changes), s = [i.range], o = hn(i.effects);
    for (let l = 1; l < t.ranges.length; l++) {
      let a = e(t.ranges[l]), f = this.changes(a.changes), u = f.map(r);
      for (let y = 0; y < l; y++)
        s[y] = s[y].map(u);
      let g = r.mapDesc(f, !0);
      s.push(a.range.map(g)), r = r.compose(u), o = ne.mapEffects(o, u).concat(ne.mapEffects(hn(a.effects), g));
    }
    return {
      changes: r,
      selection: E.create(s, t.mainIndex),
      effects: o
    };
  }
  /**
  Create a [change set](https://codemirror.net/6/docs/ref/#state.ChangeSet) from the given change
  description, taking the state's document length and line
  separator into account.
  */
  changes(e = []) {
    return e instanceof Fe ? e : Fe.of(e, this.doc.length, this.facet(pe.lineSeparator));
  }
  /**
  Using the state's [line
  separator](https://codemirror.net/6/docs/ref/#state.EditorState^lineSeparator), create a
  [`Text`](https://codemirror.net/6/docs/ref/#state.Text) instance from the given string.
  */
  toText(e) {
    return ge.of(e.split(this.facet(pe.lineSeparator) || Vo));
  }
  /**
  Return the given range of the document as a string.
  */
  sliceDoc(e = 0, t = this.doc.length) {
    return this.doc.sliceString(e, t, this.lineBreak);
  }
  /**
  Get the value of a state [facet](https://codemirror.net/6/docs/ref/#state.Facet).
  */
  facet(e) {
    let t = this.config.address[e.id];
    return t == null ? e.default : (qn(this, t), is(this, t));
  }
  /**
  Convert this state to a JSON-serializable object. When custom
  fields should be serialized, you can pass them in as an object
  mapping property names (in the resulting object, which should
  not use `doc` or `selection`) to fields.
  */
  toJSON(e) {
    let t = {
      doc: this.sliceDoc(),
      selection: this.selection.toJSON()
    };
    if (e)
      for (let i in e) {
        let r = e[i];
        r instanceof $e && this.config.address[r.id] != null && (t[i] = r.spec.toJSON(this.field(e[i]), this));
      }
    return t;
  }
  /**
  Deserialize a state from its JSON representation. When custom
  fields should be deserialized, pass the same object you passed
  to [`toJSON`](https://codemirror.net/6/docs/ref/#state.EditorState.toJSON) when serializing as
  third argument.
  */
  static fromJSON(e, t = {}, i) {
    if (!e || typeof e.doc != "string")
      throw new RangeError("Invalid JSON representation for EditorState");
    let r = [];
    if (i) {
      for (let s in i)
        if (Object.prototype.hasOwnProperty.call(e, s)) {
          let o = i[s], l = e[s];
          r.push(o.init((a) => o.spec.fromJSON(l, a)));
        }
    }
    return pe.create({
      doc: e.doc,
      selection: E.fromJSON(e.selection),
      extensions: t.extensions ? r.concat([t.extensions]) : r
    });
  }
  /**
  Create a new state. You'll usually only need this when
  initializing an editor—updated states are created by applying
  transactions.
  */
  static create(e = {}) {
    let t = ts.resolve(e.extensions || [], /* @__PURE__ */ new Map()), i = e.doc instanceof ge ? e.doc : ge.of((e.doc || "").split(t.staticFacet(pe.lineSeparator) || Vo)), r = e.selection ? e.selection instanceof E ? e.selection : E.single(e.selection.anchor, e.selection.head) : E.single(0);
    return Rf(r, i.length), t.staticFacet(Ko) || (r = r.asSingle()), new pe(t, i, r, t.dynamicSlots.map(() => null), (s, o) => o.create(s), null);
  }
  /**
  The size (in columns) of a tab in the document, determined by
  the [`tabSize`](https://codemirror.net/6/docs/ref/#state.EditorState^tabSize) facet.
  */
  get tabSize() {
    return this.facet(pe.tabSize);
  }
  /**
  Get the proper [line-break](https://codemirror.net/6/docs/ref/#state.EditorState^lineSeparator)
  string for this state.
  */
  get lineBreak() {
    return this.facet(pe.lineSeparator) || `
`;
  }
  /**
  Returns true when the editor is
  [configured](https://codemirror.net/6/docs/ref/#state.EditorState^readOnly) to be read-only.
  */
  get readOnly() {
    return this.facet(Ff);
  }
  /**
  Look up a translation for the given phrase (via the
  [`phrases`](https://codemirror.net/6/docs/ref/#state.EditorState^phrases) facet), or return the
  original string if no translation is found.
  
  If additional arguments are passed, they will be inserted in
  place of markers like `$1` (for the first value) and `$2`, etc.
  A single `$` is equivalent to `$1`, and `$$` will produce a
  literal dollar sign.
  */
  phrase(e, ...t) {
    for (let i of this.facet(pe.phrases))
      if (Object.prototype.hasOwnProperty.call(i, e)) {
        e = i[e];
        break;
      }
    return t.length && (e = e.replace(/\$(\$|\d*)/g, (i, r) => {
      if (r == "$")
        return "$";
      let s = +(r || 1);
      return !s || s > t.length ? i : t[s - 1];
    })), e;
  }
  /**
  Find the values for a given language data field, provided by the
  the [`languageData`](https://codemirror.net/6/docs/ref/#state.EditorState^languageData) facet.
  
  Examples of language data fields are...
  
  - [`"commentTokens"`](https://codemirror.net/6/docs/ref/#commands.CommentTokens) for specifying
    comment syntax.
  - [`"autocomplete"`](https://codemirror.net/6/docs/ref/#autocomplete.autocompletion^config.override)
    for providing language-specific completion sources.
  - [`"wordChars"`](https://codemirror.net/6/docs/ref/#state.EditorState.charCategorizer) for adding
    characters that should be considered part of words in this
    language.
  - [`"closeBrackets"`](https://codemirror.net/6/docs/ref/#autocomplete.CloseBracketConfig) controls
    bracket closing behavior.
  */
  languageDataAt(e, t, i = -1) {
    let r = [];
    for (let s of this.facet(Df))
      for (let o of s(this, t, i))
        Object.prototype.hasOwnProperty.call(o, e) && r.push(o[e]);
    return r;
  }
  /**
  Return a function that can categorize strings (expected to
  represent a single [grapheme cluster](https://codemirror.net/6/docs/ref/#state.findClusterBreak))
  into one of:
  
   - Word (contains an alphanumeric character or a character
     explicitly listed in the local language's `"wordChars"`
     language data, which should be a string)
   - Space (contains only whitespace)
   - Other (anything else)
  */
  charCategorizer(e) {
    let t = this.languageDataAt("wordChars", e);
    return _g(t.length ? t[0] : "");
  }
  /**
  Find the word at the given position, meaning the range
  containing all [word](https://codemirror.net/6/docs/ref/#state.CharCategory.Word) characters
  around it. If no word characters are adjacent to the position,
  this returns null.
  */
  wordAt(e) {
    let { text: t, from: i, length: r } = this.doc.lineAt(e), s = this.charCategorizer(e), o = e - i, l = e - i;
    for (; o > 0; ) {
      let a = We(t, o, !1);
      if (s(t.slice(a, o)) != Me.Word)
        break;
      o = a;
    }
    for (; l < r; ) {
      let a = We(t, l);
      if (s(t.slice(l, a)) != Me.Word)
        break;
      l = a;
    }
    return o == l ? null : E.range(o + i, l + i);
  }
}
pe.allowMultipleSelections = Ko;
pe.tabSize = /* @__PURE__ */ U.define({
  combine: (n) => n.length ? n[0] : 4
});
pe.lineSeparator = Bf;
pe.readOnly = Ff;
pe.phrases = /* @__PURE__ */ U.define({
  compare(n, e) {
    let t = Object.keys(n), i = Object.keys(e);
    return t.length == i.length && t.every((r) => n[r] == e[r]);
  }
});
pe.languageData = Df;
pe.changeFilter = Ef;
pe.transactionFilter = If;
pe.transactionExtender = Nf;
Ds.reconfigure = /* @__PURE__ */ ne.define();
function ti(n, e, t = {}) {
  let i = {};
  for (let r of n)
    for (let s of Object.keys(r)) {
      let o = r[s], l = i[s];
      if (l === void 0)
        i[s] = o;
      else if (!(l === o || o === void 0)) if (Object.hasOwnProperty.call(t, s))
        i[s] = t[s](l, o);
      else
        throw new Error("Config merge conflict for field " + s);
    }
  for (let r in e)
    i[r] === void 0 && (i[r] = e[r]);
  return i;
}
class ki {
  /**
  Compare this value with another value. Used when comparing
  rangesets. The default implementation compares by identity.
  Unless you are only creating a fixed number of unique instances
  of your value type, it is a good idea to implement this
  properly.
  */
  eq(e) {
    return this == e;
  }
  /**
  Create a [range](https://codemirror.net/6/docs/ref/#state.Range) with this value.
  */
  range(e, t = e) {
    return Uo.create(e, t, this);
  }
}
ki.prototype.startSide = ki.prototype.endSide = 0;
ki.prototype.point = !1;
ki.prototype.mapMode = Xe.TrackDel;
function zl(n, e) {
  return n == e || n.constructor == e.constructor && n.eq(e);
}
let Uo = class Vf {
  constructor(e, t, i) {
    this.from = e, this.to = t, this.value = i;
  }
  /**
  @internal
  */
  static create(e, t, i) {
    return new Vf(e, t, i);
  }
};
function Xo(n, e) {
  return n.from - e.from || n.value.startSide - e.value.startSide;
}
class $l {
  constructor(e, t, i, r) {
    this.from = e, this.to = t, this.value = i, this.maxPoint = r;
  }
  get length() {
    return this.to[this.to.length - 1];
  }
  // Find the index of the given position and side. Use the ranges'
  // `from` pos when `end == false`, `to` when `end == true`.
  findIndex(e, t, i, r = 0) {
    let s = i ? this.to : this.from;
    for (let o = r, l = s.length; ; ) {
      if (o == l)
        return o;
      let a = o + l >> 1, f = s[a] - e || (i ? this.value[a].endSide : this.value[a].startSide) - t;
      if (a == o)
        return f >= 0 ? o : l;
      f >= 0 ? l = a : o = a + 1;
    }
  }
  between(e, t, i, r) {
    for (let s = this.findIndex(t, -1e9, !0), o = this.findIndex(i, 1e9, !1, s); s < o; s++)
      if (r(this.from[s] + e, this.to[s] + e, this.value[s]) === !1)
        return !1;
  }
  map(e, t) {
    let i = [], r = [], s = [], o = -1, l = -1;
    for (let a = 0; a < this.value.length; a++) {
      let f = this.value[a], u = this.from[a] + e, g = this.to[a] + e, y, b;
      if (u == g) {
        let k = t.mapPos(u, f.startSide, f.mapMode);
        if (k == null || (y = b = k, f.startSide != f.endSide && (b = t.mapPos(u, f.endSide), b < y)))
          continue;
      } else if (y = t.mapPos(u, f.startSide), b = t.mapPos(g, f.endSide), y > b || y == b && f.startSide > 0 && f.endSide <= 0)
        continue;
      (b - y || f.endSide - f.startSide) < 0 || (o < 0 && (o = y), f.point && (l = Math.max(l, b - y)), i.push(f), r.push(y - o), s.push(b - o));
    }
    return { mapped: i.length ? new $l(r, s, i, l) : null, pos: o };
  }
}
class ce {
  constructor(e, t, i, r) {
    this.chunkPos = e, this.chunk = t, this.nextLayer = i, this.maxPoint = r;
  }
  /**
  @internal
  */
  static create(e, t, i, r) {
    return new ce(e, t, i, r);
  }
  /**
  @internal
  */
  get length() {
    let e = this.chunk.length - 1;
    return e < 0 ? 0 : Math.max(this.chunkEnd(e), this.nextLayer.length);
  }
  /**
  The number of ranges in the set.
  */
  get size() {
    if (this.isEmpty)
      return 0;
    let e = this.nextLayer.size;
    for (let t of this.chunk)
      e += t.value.length;
    return e;
  }
  /**
  @internal
  */
  chunkEnd(e) {
    return this.chunkPos[e] + this.chunk[e].length;
  }
  /**
  Update the range set, optionally adding new ranges or filtering
  out existing ones.
  
  (Note: The type parameter is just there as a kludge to work
  around TypeScript variance issues that prevented `RangeSet<X>`
  from being a subtype of `RangeSet<Y>` when `X` is a subtype of
  `Y`.)
  */
  update(e) {
    let { add: t = [], sort: i = !1, filterFrom: r = 0, filterTo: s = this.length } = e, o = e.filter;
    if (t.length == 0 && !o)
      return this;
    if (i && (t = t.slice().sort(Xo)), this.isEmpty)
      return t.length ? ce.of(t) : this;
    let l = new Hf(this, null, -1).goto(0), a = 0, f = [], u = new ei();
    for (; l.value || a < t.length; )
      if (a < t.length && (l.from - t[a].from || l.startSide - t[a].value.startSide) >= 0) {
        let g = t[a++];
        u.addInner(g.from, g.to, g.value) || f.push(g);
      } else l.rangeIndex == 1 && l.chunkIndex < this.chunk.length && (a == t.length || this.chunkEnd(l.chunkIndex) < t[a].from) && (!o || r > this.chunkEnd(l.chunkIndex) || s < this.chunkPos[l.chunkIndex]) && u.addChunk(this.chunkPos[l.chunkIndex], this.chunk[l.chunkIndex]) ? l.nextChunk() : ((!o || r > l.to || s < l.from || o(l.from, l.to, l.value)) && (u.addInner(l.from, l.to, l.value) || f.push(Uo.create(l.from, l.to, l.value))), l.next());
    return u.finishInner(this.nextLayer.isEmpty && !f.length ? ce.empty : this.nextLayer.update({ add: f, filter: o, filterFrom: r, filterTo: s }));
  }
  /**
  Map this range set through a set of changes, return the new set.
  */
  map(e) {
    if (e.empty || this.isEmpty)
      return this;
    let t = [], i = [], r = -1;
    for (let o = 0; o < this.chunk.length; o++) {
      let l = this.chunkPos[o], a = this.chunk[o], f = e.touchesRange(l, l + a.length);
      if (f === !1)
        r = Math.max(r, a.maxPoint), t.push(a), i.push(e.mapPos(l));
      else if (f === !0) {
        let { mapped: u, pos: g } = a.map(l, e);
        u && (r = Math.max(r, u.maxPoint), t.push(u), i.push(g));
      }
    }
    let s = this.nextLayer.map(e);
    return t.length == 0 ? s : new ce(i, t, s || ce.empty, r);
  }
  /**
  Iterate over the ranges that touch the region `from` to `to`,
  calling `f` for each. There is no guarantee that the ranges will
  be reported in any specific order. When the callback returns
  `false`, iteration stops.
  */
  between(e, t, i) {
    if (!this.isEmpty) {
      for (let r = 0; r < this.chunk.length; r++) {
        let s = this.chunkPos[r], o = this.chunk[r];
        if (t >= s && e <= s + o.length && o.between(s, e - s, t - s, i) === !1)
          return;
      }
      this.nextLayer.between(e, t, i);
    }
  }
  /**
  Iterate over the ranges in this set, in order, including all
  ranges that end at or after `from`.
  */
  iter(e = 0) {
    return Yn.from([this]).goto(e);
  }
  /**
  @internal
  */
  get isEmpty() {
    return this.nextLayer == this;
  }
  /**
  Iterate over the ranges in a collection of sets, in order,
  starting from `from`.
  */
  static iter(e, t = 0) {
    return Yn.from(e).goto(t);
  }
  /**
  Iterate over two groups of sets, calling methods on `comparator`
  to notify it of possible differences.
  */
  static compare(e, t, i, r, s = -1) {
    let o = e.filter((g) => g.maxPoint > 0 || !g.isEmpty && g.maxPoint >= s), l = t.filter((g) => g.maxPoint > 0 || !g.isEmpty && g.maxPoint >= s), a = gh(o, l, i), f = new En(o, a, s), u = new En(l, a, s);
    i.iterGaps((g, y, b) => mh(f, g, u, y, b, r)), i.empty && i.length == 0 && mh(f, 0, u, 0, 0, r);
  }
  /**
  Compare the contents of two groups of range sets, returning true
  if they are equivalent in the given range.
  */
  static eq(e, t, i = 0, r) {
    r == null && (r = 999999999);
    let s = e.filter((u) => !u.isEmpty && t.indexOf(u) < 0), o = t.filter((u) => !u.isEmpty && e.indexOf(u) < 0);
    if (s.length != o.length)
      return !1;
    if (!s.length)
      return !0;
    let l = gh(s, o), a = new En(s, l, 0).goto(i), f = new En(o, l, 0).goto(i);
    for (; ; ) {
      if (a.to != f.to || !Yo(a.active, f.active) || a.point && (!f.point || !zl(a.point, f.point)))
        return !1;
      if (a.to > r)
        return !0;
      a.next(), f.next();
    }
  }
  /**
  Iterate over a group of range sets at the same time, notifying
  the iterator about the ranges covering every given piece of
  content. Returns the open count (see
  [`SpanIterator.span`](https://codemirror.net/6/docs/ref/#state.SpanIterator.span)) at the end
  of the iteration.
  */
  static spans(e, t, i, r, s = -1) {
    let o = new En(e, null, s).goto(t), l = t, a = o.openStart;
    for (; ; ) {
      let f = Math.min(o.to, i);
      if (o.point) {
        let u = o.activeForPoint(o.to), g = o.pointFrom < t ? u.length + 1 : o.point.startSide < 0 ? u.length : Math.min(u.length, a);
        r.point(l, f, o.point, u, g, o.pointRank), a = Math.min(o.openEnd(f), u.length);
      } else f > l && (r.span(l, f, o.active, a), a = o.openEnd(f));
      if (o.to > i)
        return a + (o.point && o.to > i ? 1 : 0);
      l = o.to, o.next();
    }
  }
  /**
  Create a range set for the given range or array of ranges. By
  default, this expects the ranges to be _sorted_ (by start
  position and, if two start at the same position,
  `value.startSide`). You can pass `true` as second argument to
  cause the method to sort them.
  */
  static of(e, t = !1) {
    let i = new ei();
    for (let r of e instanceof Uo ? [e] : t ? jg(e) : e)
      i.add(r.from, r.to, r.value);
    return i.finish();
  }
  /**
  Join an array of range sets into a single set.
  */
  static join(e) {
    if (!e.length)
      return ce.empty;
    let t = e[e.length - 1];
    for (let i = e.length - 2; i >= 0; i--)
      for (let r = e[i]; r != ce.empty; r = r.nextLayer)
        t = new ce(r.chunkPos, r.chunk, t, Math.max(r.maxPoint, t.maxPoint));
    return t;
  }
}
ce.empty = /* @__PURE__ */ new ce([], [], null, -1);
function jg(n) {
  if (n.length > 1)
    for (let e = n[0], t = 1; t < n.length; t++) {
      let i = n[t];
      if (Xo(e, i) > 0)
        return n.slice().sort(Xo);
      e = i;
    }
  return n;
}
ce.empty.nextLayer = ce.empty;
class ei {
  finishChunk(e) {
    this.chunks.push(new $l(this.from, this.to, this.value, this.maxPoint)), this.chunkPos.push(this.chunkStart), this.chunkStart = -1, this.setMaxPoint = Math.max(this.setMaxPoint, this.maxPoint), this.maxPoint = -1, e && (this.from = [], this.to = [], this.value = []);
  }
  /**
  Create an empty builder.
  */
  constructor() {
    this.chunks = [], this.chunkPos = [], this.chunkStart = -1, this.last = null, this.lastFrom = -1e9, this.lastTo = -1e9, this.from = [], this.to = [], this.value = [], this.maxPoint = -1, this.setMaxPoint = -1, this.nextLayer = null;
  }
  /**
  Add a range. Ranges should be added in sorted (by `from` and
  `value.startSide`) order.
  */
  add(e, t, i) {
    this.addInner(e, t, i) || (this.nextLayer || (this.nextLayer = new ei())).add(e, t, i);
  }
  /**
  @internal
  */
  addInner(e, t, i) {
    let r = e - this.lastTo || i.startSide - this.last.endSide;
    if (r <= 0 && (e - this.lastFrom || i.startSide - this.last.startSide) < 0)
      throw new Error("Ranges must be added sorted by `from` position and `startSide`");
    return r < 0 ? !1 : (this.from.length == 250 && this.finishChunk(!0), this.chunkStart < 0 && (this.chunkStart = e), this.from.push(e - this.chunkStart), this.to.push(t - this.chunkStart), this.last = i, this.lastFrom = e, this.lastTo = t, this.value.push(i), i.point && (this.maxPoint = Math.max(this.maxPoint, t - e)), !0);
  }
  /**
  @internal
  */
  addChunk(e, t) {
    if ((e - this.lastTo || t.value[0].startSide - this.last.endSide) < 0)
      return !1;
    this.from.length && this.finishChunk(!0), this.setMaxPoint = Math.max(this.setMaxPoint, t.maxPoint), this.chunks.push(t), this.chunkPos.push(e);
    let i = t.value.length - 1;
    return this.last = t.value[i], this.lastFrom = t.from[i] + e, this.lastTo = t.to[i] + e, !0;
  }
  /**
  Finish the range set. Returns the new set. The builder can't be
  used anymore after this has been called.
  */
  finish() {
    return this.finishInner(ce.empty);
  }
  /**
  @internal
  */
  finishInner(e) {
    if (this.from.length && this.finishChunk(!1), this.chunks.length == 0)
      return e;
    let t = ce.create(this.chunkPos, this.chunks, this.nextLayer ? this.nextLayer.finishInner(e) : e, this.setMaxPoint);
    return this.from = null, t;
  }
}
function gh(n, e, t) {
  let i = /* @__PURE__ */ new Map();
  for (let s of n)
    for (let o = 0; o < s.chunk.length; o++)
      s.chunk[o].maxPoint <= 0 && i.set(s.chunk[o], s.chunkPos[o]);
  let r = /* @__PURE__ */ new Set();
  for (let s of e)
    for (let o = 0; o < s.chunk.length; o++) {
      let l = i.get(s.chunk[o]);
      l != null && (t ? t.mapPos(l) : l) == s.chunkPos[o] && !t?.touchesRange(l, l + s.chunk[o].length) && r.add(s.chunk[o]);
    }
  return r;
}
class Hf {
  constructor(e, t, i, r = 0) {
    this.layer = e, this.skip = t, this.minPoint = i, this.rank = r;
  }
  get startSide() {
    return this.value ? this.value.startSide : 0;
  }
  get endSide() {
    return this.value ? this.value.endSide : 0;
  }
  goto(e, t = -1e9) {
    return this.chunkIndex = this.rangeIndex = 0, this.gotoInner(e, t, !1), this;
  }
  gotoInner(e, t, i) {
    for (; this.chunkIndex < this.layer.chunk.length; ) {
      let r = this.layer.chunk[this.chunkIndex];
      if (!(this.skip && this.skip.has(r) || this.layer.chunkEnd(this.chunkIndex) < e || r.maxPoint < this.minPoint))
        break;
      this.chunkIndex++, i = !1;
    }
    if (this.chunkIndex < this.layer.chunk.length) {
      let r = this.layer.chunk[this.chunkIndex].findIndex(e - this.layer.chunkPos[this.chunkIndex], t, !0);
      (!i || this.rangeIndex < r) && this.setRangeIndex(r);
    }
    this.next();
  }
  forward(e, t) {
    (this.to - e || this.endSide - t) < 0 && this.gotoInner(e, t, !0);
  }
  next() {
    for (; ; )
      if (this.chunkIndex == this.layer.chunk.length) {
        this.from = this.to = 1e9, this.value = null;
        break;
      } else {
        let e = this.layer.chunkPos[this.chunkIndex], t = this.layer.chunk[this.chunkIndex], i = e + t.from[this.rangeIndex];
        if (this.from = i, this.to = e + t.to[this.rangeIndex], this.value = t.value[this.rangeIndex], this.setRangeIndex(this.rangeIndex + 1), this.minPoint < 0 || this.value.point && this.to - this.from >= this.minPoint)
          break;
      }
  }
  setRangeIndex(e) {
    if (e == this.layer.chunk[this.chunkIndex].value.length) {
      if (this.chunkIndex++, this.skip)
        for (; this.chunkIndex < this.layer.chunk.length && this.skip.has(this.layer.chunk[this.chunkIndex]); )
          this.chunkIndex++;
      this.rangeIndex = 0;
    } else
      this.rangeIndex = e;
  }
  nextChunk() {
    this.chunkIndex++, this.rangeIndex = 0, this.next();
  }
  compare(e) {
    return this.from - e.from || this.startSide - e.startSide || this.rank - e.rank || this.to - e.to || this.endSide - e.endSide;
  }
}
class Yn {
  constructor(e) {
    this.heap = e;
  }
  static from(e, t = null, i = -1) {
    let r = [];
    for (let s = 0; s < e.length; s++)
      for (let o = e[s]; !o.isEmpty; o = o.nextLayer)
        o.maxPoint >= i && r.push(new Hf(o, t, i, s));
    return r.length == 1 ? r[0] : new Yn(r);
  }
  get startSide() {
    return this.value ? this.value.startSide : 0;
  }
  goto(e, t = -1e9) {
    for (let i of this.heap)
      i.goto(e, t);
    for (let i = this.heap.length >> 1; i >= 0; i--)
      lo(this.heap, i);
    return this.next(), this;
  }
  forward(e, t) {
    for (let i of this.heap)
      i.forward(e, t);
    for (let i = this.heap.length >> 1; i >= 0; i--)
      lo(this.heap, i);
    (this.to - e || this.value.endSide - t) < 0 && this.next();
  }
  next() {
    if (this.heap.length == 0)
      this.from = this.to = 1e9, this.value = null, this.rank = -1;
    else {
      let e = this.heap[0];
      this.from = e.from, this.to = e.to, this.value = e.value, this.rank = e.rank, e.value && e.next(), lo(this.heap, 0);
    }
  }
}
function lo(n, e) {
  for (let t = n[e]; ; ) {
    let i = (e << 1) + 1;
    if (i >= n.length)
      break;
    let r = n[i];
    if (i + 1 < n.length && r.compare(n[i + 1]) >= 0 && (r = n[i + 1], i++), t.compare(r) < 0)
      break;
    n[i] = t, n[e] = r, e = i;
  }
}
class En {
  constructor(e, t, i) {
    this.minPoint = i, this.active = [], this.activeTo = [], this.activeRank = [], this.minActive = -1, this.point = null, this.pointFrom = 0, this.pointRank = 0, this.to = -1e9, this.endSide = 0, this.openStart = -1, this.cursor = Yn.from(e, t, i);
  }
  goto(e, t = -1e9) {
    return this.cursor.goto(e, t), this.active.length = this.activeTo.length = this.activeRank.length = 0, this.minActive = -1, this.to = e, this.endSide = t, this.openStart = -1, this.next(), this;
  }
  forward(e, t) {
    for (; this.minActive > -1 && (this.activeTo[this.minActive] - e || this.active[this.minActive].endSide - t) < 0; )
      this.removeActive(this.minActive);
    this.cursor.forward(e, t);
  }
  removeActive(e) {
    wr(this.active, e), wr(this.activeTo, e), wr(this.activeRank, e), this.minActive = vh(this.active, this.activeTo);
  }
  addActive(e) {
    let t = 0, { value: i, to: r, rank: s } = this.cursor;
    for (; t < this.activeRank.length && (s - this.activeRank[t] || r - this.activeTo[t]) > 0; )
      t++;
    Sr(this.active, t, i), Sr(this.activeTo, t, r), Sr(this.activeRank, t, s), e && Sr(e, t, this.cursor.from), this.minActive = vh(this.active, this.activeTo);
  }
  // After calling this, if `this.point` != null, the next range is a
  // point. Otherwise, it's a regular range, covered by `this.active`.
  next() {
    let e = this.to, t = this.point;
    this.point = null;
    let i = this.openStart < 0 ? [] : null;
    for (; ; ) {
      let r = this.minActive;
      if (r > -1 && (this.activeTo[r] - this.cursor.from || this.active[r].endSide - this.cursor.startSide) < 0) {
        if (this.activeTo[r] > e) {
          this.to = this.activeTo[r], this.endSide = this.active[r].endSide;
          break;
        }
        this.removeActive(r), i && wr(i, r);
      } else if (this.cursor.value)
        if (this.cursor.from > e) {
          this.to = this.cursor.from, this.endSide = this.cursor.startSide;
          break;
        } else {
          let s = this.cursor.value;
          if (!s.point)
            this.addActive(i), this.cursor.next();
          else if (t && this.cursor.to == this.to && this.cursor.from < this.cursor.to)
            this.cursor.next();
          else {
            this.point = s, this.pointFrom = this.cursor.from, this.pointRank = this.cursor.rank, this.to = this.cursor.to, this.endSide = s.endSide, this.cursor.next(), this.forward(this.to, this.endSide);
            break;
          }
        }
      else {
        this.to = this.endSide = 1e9;
        break;
      }
    }
    if (i) {
      this.openStart = 0;
      for (let r = i.length - 1; r >= 0 && i[r] < e; r--)
        this.openStart++;
    }
  }
  activeForPoint(e) {
    if (!this.active.length)
      return this.active;
    let t = [];
    for (let i = this.active.length - 1; i >= 0 && !(this.activeRank[i] < this.pointRank); i--)
      (this.activeTo[i] > e || this.activeTo[i] == e && this.active[i].endSide >= this.point.endSide) && t.push(this.active[i]);
    return t.reverse();
  }
  openEnd(e) {
    let t = 0;
    for (let i = this.activeTo.length - 1; i >= 0 && this.activeTo[i] > e; i--)
      t++;
    return t;
  }
}
function mh(n, e, t, i, r, s) {
  n.goto(e), t.goto(i);
  let o = i + r, l = i, a = i - e, f = !!s.boundChange;
  for (let u = !1; ; ) {
    let g = n.to + a - t.to, y = g || n.endSide - t.endSide, b = y < 0 ? n.to + a : t.to, k = Math.min(b, o);
    if (n.point || t.point ? (n.point && t.point && zl(n.point, t.point) && Yo(n.activeForPoint(n.to), t.activeForPoint(t.to)) || s.comparePoint(l, k, n.point, t.point), u = !1) : (u && s.boundChange(l), k > l && !Yo(n.active, t.active) && s.compareRange(l, k, n.active, t.active), f && k < o && (g || n.openEnd(b) != t.openEnd(b)) && (u = !0)), b > o)
      break;
    l = b, y <= 0 && n.next(), y >= 0 && t.next();
  }
}
function Yo(n, e) {
  if (n.length != e.length)
    return !1;
  for (let t = 0; t < n.length; t++)
    if (n[t] != e[t] && !zl(n[t], e[t]))
      return !1;
  return !0;
}
function wr(n, e) {
  for (let t = e, i = n.length - 1; t < i; t++)
    n[t] = n[t + 1];
  n.pop();
}
function Sr(n, e, t) {
  for (let i = n.length - 1; i >= e; i--)
    n[i + 1] = n[i];
  n[e] = t;
}
function vh(n, e) {
  let t = -1, i = 1e9;
  for (let r = 0; r < e.length; r++)
    (e[r] - i || n[r].endSide - n[t].endSide) < 0 && (t = r, i = e[r]);
  return t;
}
function Mn(n, e, t = n.length) {
  let i = 0;
  for (let r = 0; r < t && r < n.length; )
    n.charCodeAt(r) == 9 ? (i += e - i % e, r++) : (i++, r = We(n, r));
  return i;
}
function Go(n, e, t, i) {
  for (let r = 0, s = 0; ; ) {
    if (s >= e)
      return r;
    if (r == n.length)
      break;
    s += n.charCodeAt(r) == 9 ? t - s % t : 1, r = We(n, r);
  }
  return i === !0 ? -1 : n.length;
}
const Zo = "ͼ", yh = typeof Symbol > "u" ? "__" + Zo : Symbol.for(Zo), Jo = typeof Symbol > "u" ? "__styleSet" + Math.floor(Math.random() * 1e8) : /* @__PURE__ */ Symbol("styleSet"), bh = typeof globalThis < "u" ? globalThis : typeof window < "u" ? window : {};
class wi {
  // :: (Object<Style>, ?{finish: ?(string) → string})
  // Create a style module from the given spec.
  //
  // When `finish` is given, it is called on regular (non-`@`)
  // selectors (after `&` expansion) to compute the final selector.
  constructor(e, t) {
    this.rules = [];
    let { finish: i } = t || {};
    function r(o) {
      return /^@/.test(o) ? [o] : o.split(/,\s*/);
    }
    function s(o, l, a, f) {
      let u = [], g = /^@(\w+)\b/.exec(o[0]), y = g && g[1] == "keyframes";
      if (g && l == null) return a.push(o[0] + ";");
      for (let b in l) {
        let k = l[b];
        if (/&/.test(b))
          s(
            b.split(/,\s*/).map((C) => o.map((M) => C.replace(/&/, M))).reduce((C, M) => C.concat(M)),
            k,
            a
          );
        else if (k && typeof k == "object") {
          if (!g) throw new RangeError("The value of a property (" + b + ") should be a primitive value.");
          s(r(b), k, u, y);
        } else k != null && u.push(b.replace(/_.*/, "").replace(/[A-Z]/g, (C) => "-" + C.toLowerCase()) + ": " + k + ";");
      }
      (u.length || y) && a.push((i && !g && !f ? o.map(i) : o).join(", ") + " {" + u.join(" ") + "}");
    }
    for (let o in e) s(r(o), e[o], this.rules);
  }
  // :: () → string
  // Returns a string containing the module's CSS rules.
  getRules() {
    return this.rules.join(`
`);
  }
  // :: () → string
  // Generate a new unique CSS class name.
  static newName() {
    let e = bh[yh] || 1;
    return bh[yh] = e + 1, Zo + e.toString(36);
  }
  // :: (union<Document, ShadowRoot>, union<[StyleModule], StyleModule>, ?{nonce: ?string})
  //
  // Mount the given set of modules in the given DOM root, which ensures
  // that the CSS rules defined by the module are available in that
  // context.
  //
  // Rules are only added to the document once per root.
  //
  // Rule order will follow the order of the modules, so that rules from
  // modules later in the array take precedence of those from earlier
  // modules. If you call this function multiple times for the same root
  // in a way that changes the order of already mounted modules, the old
  // order will be changed.
  //
  // If a Content Security Policy nonce is provided, it is added to
  // the `<style>` tag generated by the library.
  static mount(e, t, i) {
    let r = e[Jo], s = i && i.nonce;
    r ? s && r.setNonce(s) : r = new Ug(e, s), r.mount(Array.isArray(t) ? t : [t], e);
  }
}
let xh = /* @__PURE__ */ new Map();
class Ug {
  constructor(e, t) {
    let i = e.ownerDocument || e, r = i.defaultView;
    if (!e.head && e.adoptedStyleSheets && r.CSSStyleSheet) {
      let s = xh.get(i);
      if (s) return e[Jo] = s;
      this.sheet = new r.CSSStyleSheet(), xh.set(i, this);
    } else
      this.styleTag = i.createElement("style"), t && this.styleTag.setAttribute("nonce", t);
    this.modules = [], e[Jo] = this;
  }
  mount(e, t) {
    let i = this.sheet, r = 0, s = 0;
    for (let o = 0; o < e.length; o++) {
      let l = e[o], a = this.modules.indexOf(l);
      if (a < s && a > -1 && (this.modules.splice(a, 1), s--, a = -1), a == -1) {
        if (this.modules.splice(s++, 0, l), i) for (let f = 0; f < l.rules.length; f++)
          i.insertRule(l.rules[f], r++);
      } else {
        for (; s < a; ) r += this.modules[s++].rules.length;
        r += l.rules.length, s++;
      }
    }
    if (i)
      t.adoptedStyleSheets.indexOf(this.sheet) < 0 && (t.adoptedStyleSheets = [this.sheet, ...t.adoptedStyleSheets]);
    else {
      let o = "";
      for (let a = 0; a < this.modules.length; a++)
        o += this.modules[a].getRules() + `
`;
      this.styleTag.textContent = o;
      let l = t.head || t;
      this.styleTag.parentNode != l && l.insertBefore(this.styleTag, l.firstChild);
    }
  }
  setNonce(e) {
    this.styleTag && this.styleTag.getAttribute("nonce") != e && this.styleTag.setAttribute("nonce", e);
  }
}
var Si = {
  8: "Backspace",
  9: "Tab",
  10: "Enter",
  12: "NumLock",
  13: "Enter",
  16: "Shift",
  17: "Control",
  18: "Alt",
  20: "CapsLock",
  27: "Escape",
  32: " ",
  33: "PageUp",
  34: "PageDown",
  35: "End",
  36: "Home",
  37: "ArrowLeft",
  38: "ArrowUp",
  39: "ArrowRight",
  40: "ArrowDown",
  44: "PrintScreen",
  45: "Insert",
  46: "Delete",
  59: ";",
  61: "=",
  91: "Meta",
  92: "Meta",
  106: "*",
  107: "+",
  108: ",",
  109: "-",
  110: ".",
  111: "/",
  144: "NumLock",
  145: "ScrollLock",
  160: "Shift",
  161: "Shift",
  162: "Control",
  163: "Control",
  164: "Alt",
  165: "Alt",
  173: "-",
  186: ";",
  187: "=",
  188: ",",
  189: "-",
  190: ".",
  191: "/",
  192: "`",
  219: "[",
  220: "\\",
  221: "]",
  222: "'"
}, Gn = {
  48: ")",
  49: "!",
  50: "@",
  51: "#",
  52: "$",
  53: "%",
  54: "^",
  55: "&",
  56: "*",
  57: "(",
  59: ":",
  61: "+",
  173: "_",
  186: ":",
  187: "+",
  188: "<",
  189: "_",
  190: ">",
  191: "?",
  192: "~",
  219: "{",
  220: "|",
  221: "}",
  222: '"'
}, Xg = typeof navigator < "u" && /Mac/.test(navigator.platform), Yg = typeof navigator < "u" && /MSIE \d|Trident\/(?:[7-9]|\d{2,})\..*rv:(\d+)/.exec(navigator.userAgent);
for (var Ue = 0; Ue < 10; Ue++) Si[48 + Ue] = Si[96 + Ue] = String(Ue);
for (var Ue = 1; Ue <= 24; Ue++) Si[Ue + 111] = "F" + Ue;
for (var Ue = 65; Ue <= 90; Ue++)
  Si[Ue] = String.fromCharCode(Ue + 32), Gn[Ue] = String.fromCharCode(Ue);
for (var ao in Si) Gn.hasOwnProperty(ao) || (Gn[ao] = Si[ao]);
function Gg(n) {
  var e = Xg && n.metaKey && n.shiftKey && !n.ctrlKey && !n.altKey || Yg && n.shiftKey && n.key && n.key.length == 1 || n.key == "Unidentified", t = !e && n.key || (n.shiftKey ? Gn : Si)[n.keyCode] || n.key || "Unidentified";
  return t == "Esc" && (t = "Escape"), t == "Del" && (t = "Delete"), t == "Left" && (t = "ArrowLeft"), t == "Up" && (t = "ArrowUp"), t == "Right" && (t = "ArrowRight"), t == "Down" && (t = "ArrowDown"), t;
}
function ye() {
  var n = arguments[0];
  typeof n == "string" && (n = document.createElement(n));
  var e = 1, t = arguments[1];
  if (t && typeof t == "object" && t.nodeType == null && !Array.isArray(t)) {
    for (var i in t) if (Object.prototype.hasOwnProperty.call(t, i)) {
      var r = t[i];
      typeof r == "string" ? n.setAttribute(i, r) : r != null && (n[i] = r);
    }
    e++;
  }
  for (; e < arguments.length; e++) zf(n, arguments[e]);
  return n;
}
function zf(n, e) {
  if (typeof e == "string")
    n.appendChild(document.createTextNode(e));
  else if (e != null) if (e.nodeType != null)
    n.appendChild(e);
  else if (Array.isArray(e))
    for (var t = 0; t < e.length; t++) zf(n, e[t]);
  else
    throw new RangeError("Unsupported child node: " + e);
}
let rt = typeof navigator < "u" ? navigator : { userAgent: "", vendor: "", platform: "" }, el = typeof document < "u" ? document : { documentElement: { style: {} } };
const tl = /* @__PURE__ */ /Edge\/(\d+)/.exec(rt.userAgent), $f = /* @__PURE__ */ /MSIE \d/.test(rt.userAgent), il = /* @__PURE__ */ /Trident\/(?:[7-9]|\d{2,})\..*rv:(\d+)/.exec(rt.userAgent), Bs = !!($f || il || tl), kh = !Bs && /* @__PURE__ */ /gecko\/(\d+)/i.test(rt.userAgent), ho = !Bs && /* @__PURE__ */ /Chrome\/(\d+)/.exec(rt.userAgent), Zg = "webkitFontSmoothing" in el.documentElement.style, nl = !Bs && /* @__PURE__ */ /Apple Computer/.test(rt.vendor), wh = nl && (/* @__PURE__ */ /Mobile\/\w+/.test(rt.userAgent) || rt.maxTouchPoints > 2);
var j = {
  mac: wh || /* @__PURE__ */ /Mac/.test(rt.platform),
  windows: /* @__PURE__ */ /Win/.test(rt.platform),
  linux: /* @__PURE__ */ /Linux|X11/.test(rt.platform),
  ie: Bs,
  ie_version: $f ? el.documentMode || 6 : il ? +il[1] : tl ? +tl[1] : 0,
  gecko: kh,
  gecko_version: kh ? +(/* @__PURE__ */ /Firefox\/(\d+)/.exec(rt.userAgent) || [0, 0])[1] : 0,
  chrome: !!ho,
  chrome_version: ho ? +ho[1] : 0,
  ios: wh,
  android: /* @__PURE__ */ /Android\b/.test(rt.userAgent),
  webkit_version: Zg ? +(/* @__PURE__ */ /\bAppleWebKit\/(\d+)/.exec(rt.userAgent) || [0, 0])[1] : 0,
  safari: nl,
  safari_version: nl ? +(/* @__PURE__ */ /\bVersion\/(\d+(\.\d+)?)/.exec(rt.userAgent) || [0, 0])[1] : 0,
  tabSize: el.documentElement.style.tabSize != null ? "tab-size" : "-moz-tab-size"
};
function ql(n, e) {
  for (let t in n)
    t == "class" && e.class ? e.class += " " + n.class : t == "style" && e.style ? e.style += ";" + n.style : e[t] = n[t];
  return e;
}
const ns = /* @__PURE__ */ Object.create(null);
function Kl(n, e, t) {
  if (n == e)
    return !0;
  n || (n = ns), e || (e = ns);
  let i = Object.keys(n), r = Object.keys(e);
  if (i.length - 0 != r.length - 0)
    return !1;
  for (let s of i)
    if (s != t && (r.indexOf(s) == -1 || n[s] !== e[s]))
      return !1;
  return !0;
}
function Jg(n, e) {
  for (let t = n.attributes.length - 1; t >= 0; t--) {
    let i = n.attributes[t].name;
    e[i] == null && n.removeAttribute(i);
  }
  for (let t in e) {
    let i = e[t];
    t == "style" ? n.style.cssText = i : n.getAttribute(t) != i && n.setAttribute(t, i);
  }
}
function Sh(n, e, t) {
  let i = !1;
  if (e)
    for (let r in e)
      t && r in t || (i = !0, r == "style" ? n.style.cssText = "" : n.removeAttribute(r));
  if (t)
    for (let r in t)
      e && e[r] == t[r] || (i = !0, r == "style" ? n.style.cssText = t[r] : n.setAttribute(r, t[r]));
  return i;
}
function em(n) {
  let e = /* @__PURE__ */ Object.create(null);
  for (let t = 0; t < n.attributes.length; t++) {
    let i = n.attributes[t];
    e[i.name] = i.value;
  }
  return e;
}
class ci {
  /**
  Compare this instance to another instance of the same type.
  (TypeScript can't express this, but only instances of the same
  specific class will be passed to this method.) This is used to
  avoid redrawing widgets when they are replaced by a new
  decoration of the same type. The default implementation just
  returns `false`, which will cause new instances of the widget to
  always be redrawn.
  */
  eq(e) {
    return !1;
  }
  /**
  Update a DOM element created by a widget of the same type (but
  different, non-`eq` content) to reflect this widget. May return
  true to indicate that it could update, false to indicate it
  couldn't (in which case the widget will be redrawn). The default
  implementation just returns false.
  */
  updateDOM(e, t) {
    return !1;
  }
  /**
  @internal
  */
  compare(e) {
    return this == e || this.constructor == e.constructor && this.eq(e);
  }
  /**
  The estimated height this widget will have, to be used when
  estimating the height of content that hasn't been drawn. May
  return -1 to indicate you don't know. The default implementation
  returns -1.
  */
  get estimatedHeight() {
    return -1;
  }
  /**
  For inline widgets that are displayed inline (as opposed to
  `inline-block`) and introduce line breaks (through `<br>` tags
  or textual newlines), this must indicate the amount of line
  breaks they introduce. Defaults to 0.
  */
  get lineBreaks() {
    return 0;
  }
  /**
  Can be used to configure which kinds of events inside the widget
  should be ignored by the editor. The default is to ignore all
  events.
  */
  ignoreEvent(e) {
    return !0;
  }
  /**
  Override the way screen coordinates for positions at/in the
  widget are found. `pos` will be the offset into the widget, and
  `side` the side of the position that is being queried—less than
  zero for before, greater than zero for after, and zero for
  directly at that position.
  */
  coordsAt(e, t, i) {
    return null;
  }
  /**
  @internal
  */
  get isHidden() {
    return !1;
  }
  /**
  @internal
  */
  get editable() {
    return !1;
  }
  /**
  This is called when the an instance of the widget is removed
  from the editor view.
  */
  destroy(e) {
  }
}
var Ye = /* @__PURE__ */ (function(n) {
  return n[n.Text = 0] = "Text", n[n.WidgetBefore = 1] = "WidgetBefore", n[n.WidgetAfter = 2] = "WidgetAfter", n[n.WidgetRange = 3] = "WidgetRange", n;
})(Ye || (Ye = {}));
class G extends ki {
  constructor(e, t, i, r) {
    super(), this.startSide = e, this.endSide = t, this.widget = i, this.spec = r;
  }
  /**
  @internal
  */
  get heightRelevant() {
    return !1;
  }
  /**
  Create a mark decoration, which influences the styling of the
  content in its range. Nested mark decorations will cause nested
  DOM elements to be created. Nesting order is determined by
  precedence of the [facet](https://codemirror.net/6/docs/ref/#view.EditorView^decorations), with
  the higher-precedence decorations creating the inner DOM nodes.
  Such elements are split on line boundaries and on the boundaries
  of lower-precedence decorations.
  */
  static mark(e) {
    return new cr(e);
  }
  /**
  Create a widget decoration, which displays a DOM element at the
  given position.
  */
  static widget(e) {
    let t = Math.max(-1e4, Math.min(1e4, e.side || 0)), i = !!e.block;
    return t += i && !e.inlineOrder ? t > 0 ? 3e8 : -4e8 : t > 0 ? 1e8 : -1e8, new qi(e, t, t, i, e.widget || null, !1);
  }
  /**
  Create a replace decoration which replaces the given range with
  a widget, or simply hides it.
  */
  static replace(e) {
    let t = !!e.block, i, r;
    if (e.isBlockGap)
      i = -5e8, r = 4e8;
    else {
      let { start: s, end: o } = qf(e, t);
      i = (s ? t ? -3e8 : -1 : 5e8) - 1, r = (o ? t ? 2e8 : 1 : -6e8) + 1;
    }
    return new qi(e, i, r, t, e.widget || null, !0);
  }
  /**
  Create a line decoration, which can add DOM attributes to the
  line starting at the given position.
  */
  static line(e) {
    return new fr(e);
  }
  /**
  Build a [`DecorationSet`](https://codemirror.net/6/docs/ref/#view.DecorationSet) from the given
  decorated range or ranges. If the ranges aren't already sorted,
  pass `true` for `sort` to make the library sort them for you.
  */
  static set(e, t = !1) {
    return ce.of(e, t);
  }
  /**
  @internal
  */
  hasHeight() {
    return this.widget ? this.widget.estimatedHeight > -1 : !1;
  }
}
G.none = ce.empty;
class cr extends G {
  constructor(e) {
    let { start: t, end: i } = qf(e);
    super(t ? -1 : 5e8, i ? 1 : -6e8, null, e), this.tagName = e.tagName || "span", this.attrs = e.class && e.attributes ? ql(e.attributes, { class: e.class }) : e.class ? { class: e.class } : e.attributes || ns;
  }
  eq(e) {
    return this == e || e instanceof cr && this.tagName == e.tagName && Kl(this.attrs, e.attrs);
  }
  range(e, t = e) {
    if (e >= t)
      throw new RangeError("Mark decorations may not be empty");
    return super.range(e, t);
  }
}
cr.prototype.point = !1;
class fr extends G {
  constructor(e) {
    super(-2e8, -2e8, null, e);
  }
  eq(e) {
    return e instanceof fr && this.spec.class == e.spec.class && Kl(this.spec.attributes, e.spec.attributes);
  }
  range(e, t = e) {
    if (t != e)
      throw new RangeError("Line decoration ranges must be zero-length");
    return super.range(e, t);
  }
}
fr.prototype.mapMode = Xe.TrackBefore;
fr.prototype.point = !0;
class qi extends G {
  constructor(e, t, i, r, s, o) {
    super(t, i, s, e), this.block = r, this.isReplace = o, this.mapMode = r ? t <= 0 ? Xe.TrackBefore : Xe.TrackAfter : Xe.TrackDel;
  }
  // Only relevant when this.block == true
  get type() {
    return this.startSide != this.endSide ? Ye.WidgetRange : this.startSide <= 0 ? Ye.WidgetBefore : Ye.WidgetAfter;
  }
  get heightRelevant() {
    return this.block || !!this.widget && (this.widget.estimatedHeight >= 5 || this.widget.lineBreaks > 0);
  }
  eq(e) {
    return e instanceof qi && tm(this.widget, e.widget) && this.block == e.block && this.startSide == e.startSide && this.endSide == e.endSide;
  }
  range(e, t = e) {
    if (this.isReplace && (e > t || e == t && this.startSide > 0 && this.endSide <= 0))
      throw new RangeError("Invalid range for replacement decoration");
    if (!this.isReplace && t != e)
      throw new RangeError("Widget decorations can only have zero-length ranges");
    return super.range(e, t);
  }
}
qi.prototype.point = !0;
function qf(n, e = !1) {
  let { inclusiveStart: t, inclusiveEnd: i } = n;
  return t == null && (t = n.inclusive), i == null && (i = n.inclusive), { start: t ?? e, end: i ?? e };
}
function tm(n, e) {
  return n == e || !!(n && e && n.compare(e));
}
function cn(n, e, t, i = 0) {
  let r = t.length - 1;
  r >= 0 && t[r] + i >= n ? t[r] = Math.max(t[r], e) : t.push(n, e);
}
class Zn extends ki {
  constructor(e, t) {
    super(), this.tagName = e, this.attributes = t;
  }
  eq(e) {
    return e == this || e instanceof Zn && this.tagName == e.tagName && Kl(this.attributes, e.attributes);
  }
  /**
  Create a block wrapper object with the given tag name and
  attributes.
  */
  static create(e) {
    return new Zn(e.tagName, e.attributes || ns);
  }
  /**
  Create a range set from the given block wrapper ranges.
  */
  static set(e, t = !1) {
    return ce.of(e, t);
  }
}
Zn.prototype.startSide = Zn.prototype.endSide = -1;
function yn(n) {
  let e;
  return n.nodeType == 11 ? e = n.getSelection ? n : n.ownerDocument : e = n, e.getSelection();
}
function rl(n, e) {
  return e ? n == e || n.contains(e.nodeType != 1 ? e.parentNode : e) : !1;
}
function Kn(n, e) {
  if (!e.anchorNode)
    return !1;
  try {
    return rl(n, e.anchorNode);
  } catch {
    return !1;
  }
}
function jr(n) {
  return n.nodeType == 3 ? Jn(n, 0, n.nodeValue.length).getClientRects() : n.nodeType == 1 ? n.getClientRects() : [];
}
function _n(n, e, t, i) {
  return t ? Ch(n, e, t, i, -1) || Ch(n, e, t, i, 1) : !1;
}
function Ci(n) {
  for (var e = 0; ; e++)
    if (n = n.previousSibling, !n)
      return e;
}
function rs(n) {
  return n.nodeType == 1 && /^(DIV|P|LI|UL|OL|BLOCKQUOTE|DD|DT|H\d|SECTION|PRE)$/.test(n.nodeName);
}
function Ch(n, e, t, i, r) {
  for (; ; ) {
    if (n == t && e == i)
      return !0;
    if (e == (r < 0 ? 0 : li(n))) {
      if (n.nodeName == "DIV")
        return !1;
      let s = n.parentNode;
      if (!s || s.nodeType != 1)
        return !1;
      e = Ci(n) + (r < 0 ? 0 : 1), n = s;
    } else if (n.nodeType == 1) {
      if (n = n.childNodes[e + (r < 0 ? -1 : 0)], n.nodeType == 1 && n.contentEditable == "false")
        return !1;
      e = r < 0 ? li(n) : 0;
    } else
      return !1;
  }
}
function li(n) {
  return n.nodeType == 3 ? n.nodeValue.length : n.childNodes.length;
}
function ss(n, e) {
  let t = e ? n.left : n.right;
  return { left: t, right: t, top: n.top, bottom: n.bottom };
}
function im(n) {
  let e = n.visualViewport;
  return e ? {
    left: 0,
    right: e.width,
    top: 0,
    bottom: e.height
  } : {
    left: 0,
    right: n.innerWidth,
    top: 0,
    bottom: n.innerHeight
  };
}
function Kf(n, e) {
  let t = e.width / n.offsetWidth, i = e.height / n.offsetHeight;
  return (t > 0.995 && t < 1.005 || !isFinite(t) || Math.abs(e.width - n.offsetWidth) < 1) && (t = 1), (i > 0.995 && i < 1.005 || !isFinite(i) || Math.abs(e.height - n.offsetHeight) < 1) && (i = 1), { scaleX: t, scaleY: i };
}
function nm(n, e, t, i, r, s, o, l) {
  let a = n.ownerDocument, f = a.defaultView || window;
  for (let u = n, g = !1; u && !g; )
    if (u.nodeType == 1) {
      let y, b = u == a.body, k = 1, C = 1;
      if (b)
        y = im(f);
      else {
        if (/^(fixed|sticky)$/.test(getComputedStyle(u).position) && (g = !0), u.scrollHeight <= u.clientHeight && u.scrollWidth <= u.clientWidth) {
          u = u.assignedSlot || u.parentNode;
          continue;
        }
        let I = u.getBoundingClientRect();
        ({ scaleX: k, scaleY: C } = Kf(u, I)), y = {
          left: I.left,
          right: I.left + u.clientWidth * k,
          top: I.top,
          bottom: I.top + u.clientHeight * C
        };
      }
      let M = 0, R = 0;
      if (r == "nearest")
        e.top < y.top ? (R = e.top - (y.top + o), t > 0 && e.bottom > y.bottom + R && (R = e.bottom - y.bottom + o)) : e.bottom > y.bottom && (R = e.bottom - y.bottom + o, t < 0 && e.top - R < y.top && (R = e.top - (y.top + o)));
      else {
        let I = e.bottom - e.top, N = y.bottom - y.top;
        R = (r == "center" && I <= N ? e.top + I / 2 - N / 2 : r == "start" || r == "center" && t < 0 ? e.top - o : e.bottom - N + o) - y.top;
      }
      if (i == "nearest" ? e.left < y.left ? (M = e.left - (y.left + s), t > 0 && e.right > y.right + M && (M = e.right - y.right + s)) : e.right > y.right && (M = e.right - y.right + s, t < 0 && e.left < y.left + M && (M = e.left - (y.left + s))) : M = (i == "center" ? e.left + (e.right - e.left) / 2 - (y.right - y.left) / 2 : i == "start" == l ? e.left - s : e.right - (y.right - y.left) + s) - y.left, M || R)
        if (b)
          f.scrollBy(M, R);
        else {
          let I = 0, N = 0;
          if (R) {
            let z = u.scrollTop;
            u.scrollTop += R / C, N = (u.scrollTop - z) * C;
          }
          if (M) {
            let z = u.scrollLeft;
            u.scrollLeft += M / k, I = (u.scrollLeft - z) * k;
          }
          e = {
            left: e.left - I,
            top: e.top - N,
            right: e.right - I,
            bottom: e.bottom - N
          }, I && Math.abs(I - M) < 1 && (i = "nearest"), N && Math.abs(N - R) < 1 && (r = "nearest");
        }
      if (b)
        break;
      (e.top < y.top || e.bottom > y.bottom || e.left < y.left || e.right > y.right) && (e = {
        left: Math.max(e.left, y.left),
        right: Math.min(e.right, y.right),
        top: Math.max(e.top, y.top),
        bottom: Math.min(e.bottom, y.bottom)
      }), u = u.assignedSlot || u.parentNode;
    } else if (u.nodeType == 11)
      u = u.host;
    else
      break;
}
function rm(n) {
  let e = n.ownerDocument, t, i;
  for (let r = n.parentNode; r && !(r == e.body || t && i); )
    if (r.nodeType == 1)
      !i && r.scrollHeight > r.clientHeight && (i = r), !t && r.scrollWidth > r.clientWidth && (t = r), r = r.assignedSlot || r.parentNode;
    else if (r.nodeType == 11)
      r = r.host;
    else
      break;
  return { x: t, y: i };
}
class sm {
  constructor() {
    this.anchorNode = null, this.anchorOffset = 0, this.focusNode = null, this.focusOffset = 0;
  }
  eq(e) {
    return this.anchorNode == e.anchorNode && this.anchorOffset == e.anchorOffset && this.focusNode == e.focusNode && this.focusOffset == e.focusOffset;
  }
  setRange(e) {
    let { anchorNode: t, focusNode: i } = e;
    this.set(t, Math.min(e.anchorOffset, t ? li(t) : 0), i, Math.min(e.focusOffset, i ? li(i) : 0));
  }
  set(e, t, i, r) {
    this.anchorNode = e, this.anchorOffset = t, this.focusNode = i, this.focusOffset = r;
  }
}
let Ii = null;
j.safari && j.safari_version >= 26 && (Ii = !1);
function _f(n) {
  if (n.setActive)
    return n.setActive();
  if (Ii)
    return n.focus(Ii);
  let e = [];
  for (let t = n; t && (e.push(t, t.scrollTop, t.scrollLeft), t != t.ownerDocument); t = t.parentNode)
    ;
  if (n.focus(Ii == null ? {
    get preventScroll() {
      return Ii = { preventScroll: !0 }, !0;
    }
  } : void 0), !Ii) {
    Ii = !1;
    for (let t = 0; t < e.length; ) {
      let i = e[t++], r = e[t++], s = e[t++];
      i.scrollTop != r && (i.scrollTop = r), i.scrollLeft != s && (i.scrollLeft = s);
    }
  }
}
let Oh;
function Jn(n, e, t = e) {
  let i = Oh || (Oh = document.createRange());
  return i.setEnd(n, t), i.setStart(n, e), i;
}
function fn(n, e, t, i) {
  let r = { key: e, code: e, keyCode: t, which: t, cancelable: !0 };
  i && ({ altKey: r.altKey, ctrlKey: r.ctrlKey, shiftKey: r.shiftKey, metaKey: r.metaKey } = i);
  let s = new KeyboardEvent("keydown", r);
  s.synthetic = !0, n.dispatchEvent(s);
  let o = new KeyboardEvent("keyup", r);
  return o.synthetic = !0, n.dispatchEvent(o), s.defaultPrevented || o.defaultPrevented;
}
function om(n) {
  for (; n; ) {
    if (n && (n.nodeType == 9 || n.nodeType == 11 && n.host))
      return n;
    n = n.assignedSlot || n.parentNode;
  }
  return null;
}
function lm(n, e) {
  let t = e.focusNode, i = e.focusOffset;
  if (!t || e.anchorNode != t || e.anchorOffset != i)
    return !1;
  for (i = Math.min(i, li(t)); ; )
    if (i) {
      if (t.nodeType != 1)
        return !1;
      let r = t.childNodes[i - 1];
      r.contentEditable == "false" ? i-- : (t = r, i = li(t));
    } else {
      if (t == n)
        return !0;
      i = Ci(t), t = t.parentNode;
    }
}
function jf(n) {
  return n.scrollTop > Math.max(1, n.scrollHeight - n.clientHeight - 4);
}
function Uf(n, e) {
  for (let t = n, i = e; ; ) {
    if (t.nodeType == 3 && i > 0)
      return { node: t, offset: i };
    if (t.nodeType == 1 && i > 0) {
      if (t.contentEditable == "false")
        return null;
      t = t.childNodes[i - 1], i = li(t);
    } else if (t.parentNode && !rs(t))
      i = Ci(t), t = t.parentNode;
    else
      return null;
  }
}
function Xf(n, e) {
  for (let t = n, i = e; ; ) {
    if (t.nodeType == 3 && i < t.nodeValue.length)
      return { node: t, offset: i };
    if (t.nodeType == 1 && i < t.childNodes.length) {
      if (t.contentEditable == "false")
        return null;
      t = t.childNodes[i], i = 0;
    } else if (t.parentNode && !rs(t))
      i = Ci(t) + 1, t = t.parentNode;
    else
      return null;
  }
}
class Nt {
  constructor(e, t, i = !0) {
    this.node = e, this.offset = t, this.precise = i;
  }
  static before(e, t) {
    return new Nt(e.parentNode, Ci(e), t);
  }
  static after(e, t) {
    return new Nt(e.parentNode, Ci(e) + 1, t);
  }
}
var xe = /* @__PURE__ */ (function(n) {
  return n[n.LTR = 0] = "LTR", n[n.RTL = 1] = "RTL", n;
})(xe || (xe = {}));
const Ki = xe.LTR, _l = xe.RTL;
function Yf(n) {
  let e = [];
  for (let t = 0; t < n.length; t++)
    e.push(1 << +n[t]);
  return e;
}
const am = /* @__PURE__ */ Yf("88888888888888888888888888888888888666888888787833333333337888888000000000000000000000000008888880000000000000000000000000088888888888888888888888888888888888887866668888088888663380888308888800000000000000000000000800000000000000000000000000000008"), hm = /* @__PURE__ */ Yf("4444448826627288999999999992222222222222222222222222222222222222222222222229999999999999999999994444444444644222822222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222999999949999999229989999223333333333"), sl = /* @__PURE__ */ Object.create(null), qt = [];
for (let n of ["()", "[]", "{}"]) {
  let e = /* @__PURE__ */ n.charCodeAt(0), t = /* @__PURE__ */ n.charCodeAt(1);
  sl[e] = t, sl[t] = -e;
}
function Gf(n) {
  return n <= 247 ? am[n] : 1424 <= n && n <= 1524 ? 2 : 1536 <= n && n <= 1785 ? hm[n - 1536] : 1774 <= n && n <= 2220 ? 4 : 8192 <= n && n <= 8204 ? 256 : 64336 <= n && n <= 65023 ? 4 : 1;
}
const cm = /[\u0590-\u05f4\u0600-\u06ff\u0700-\u08ac\ufb50-\ufdff]/;
class si {
  /**
  The direction of this span.
  */
  get dir() {
    return this.level % 2 ? _l : Ki;
  }
  /**
  @internal
  */
  constructor(e, t, i) {
    this.from = e, this.to = t, this.level = i;
  }
  /**
  @internal
  */
  side(e, t) {
    return this.dir == t == e ? this.to : this.from;
  }
  /**
  @internal
  */
  forward(e, t) {
    return e == (this.dir == t);
  }
  /**
  @internal
  */
  static find(e, t, i, r) {
    let s = -1;
    for (let o = 0; o < e.length; o++) {
      let l = e[o];
      if (l.from <= t && l.to >= t) {
        if (l.level == i)
          return o;
        (s < 0 || (r != 0 ? r < 0 ? l.from < t : l.to > t : e[s].level > l.level)) && (s = o);
      }
    }
    if (s < 0)
      throw new RangeError("Index out of range");
    return s;
  }
}
function Zf(n, e) {
  if (n.length != e.length)
    return !1;
  for (let t = 0; t < n.length; t++) {
    let i = n[t], r = e[t];
    if (i.from != r.from || i.to != r.to || i.direction != r.direction || !Zf(i.inner, r.inner))
      return !1;
  }
  return !0;
}
const ke = [];
function fm(n, e, t, i, r) {
  for (let s = 0; s <= i.length; s++) {
    let o = s ? i[s - 1].to : e, l = s < i.length ? i[s].from : t, a = s ? 256 : r;
    for (let f = o, u = a, g = a; f < l; f++) {
      let y = Gf(n.charCodeAt(f));
      y == 512 ? y = u : y == 8 && g == 4 && (y = 16), ke[f] = y == 4 ? 2 : y, y & 7 && (g = y), u = y;
    }
    for (let f = o, u = a, g = a; f < l; f++) {
      let y = ke[f];
      if (y == 128)
        f < l - 1 && u == ke[f + 1] && u & 24 ? y = ke[f] = u : ke[f] = 256;
      else if (y == 64) {
        let b = f + 1;
        for (; b < l && ke[b] == 64; )
          b++;
        let k = f && u == 8 || b < t && ke[b] == 8 ? g == 1 ? 1 : 8 : 256;
        for (let C = f; C < b; C++)
          ke[C] = k;
        f = b - 1;
      } else y == 8 && g == 1 && (ke[f] = 1);
      u = y, y & 7 && (g = y);
    }
  }
}
function um(n, e, t, i, r) {
  let s = r == 1 ? 2 : 1;
  for (let o = 0, l = 0, a = 0; o <= i.length; o++) {
    let f = o ? i[o - 1].to : e, u = o < i.length ? i[o].from : t;
    for (let g = f, y, b, k; g < u; g++)
      if (b = sl[y = n.charCodeAt(g)])
        if (b < 0) {
          for (let C = l - 3; C >= 0; C -= 3)
            if (qt[C + 1] == -b) {
              let M = qt[C + 2], R = M & 2 ? r : M & 4 ? M & 1 ? s : r : 0;
              R && (ke[g] = ke[qt[C]] = R), l = C;
              break;
            }
        } else {
          if (qt.length == 189)
            break;
          qt[l++] = g, qt[l++] = y, qt[l++] = a;
        }
      else if ((k = ke[g]) == 2 || k == 1) {
        let C = k == r;
        a = C ? 0 : 1;
        for (let M = l - 3; M >= 0; M -= 3) {
          let R = qt[M + 2];
          if (R & 2)
            break;
          if (C)
            qt[M + 2] |= 2;
          else {
            if (R & 4)
              break;
            qt[M + 2] |= 4;
          }
        }
      }
  }
}
function dm(n, e, t, i) {
  for (let r = 0, s = i; r <= t.length; r++) {
    let o = r ? t[r - 1].to : n, l = r < t.length ? t[r].from : e;
    for (let a = o; a < l; ) {
      let f = ke[a];
      if (f == 256) {
        let u = a + 1;
        for (; ; )
          if (u == l) {
            if (r == t.length)
              break;
            u = t[r++].to, l = r < t.length ? t[r].from : e;
          } else if (ke[u] == 256)
            u++;
          else
            break;
        let g = s == 1, y = (u < e ? ke[u] : i) == 1, b = g == y ? g ? 1 : 2 : i;
        for (let k = u, C = r, M = C ? t[C - 1].to : n; k > a; )
          k == M && (k = t[--C].from, M = C ? t[C - 1].to : n), ke[--k] = b;
        a = u;
      } else
        s = f, a++;
    }
  }
}
function ol(n, e, t, i, r, s, o) {
  let l = i % 2 ? 2 : 1;
  if (i % 2 == r % 2)
    for (let a = e, f = 0; a < t; ) {
      let u = !0, g = !1;
      if (f == s.length || a < s[f].from) {
        let C = ke[a];
        C != l && (u = !1, g = C == 16);
      }
      let y = !u && l == 1 ? [] : null, b = u ? i : i + 1, k = a;
      e: for (; ; )
        if (f < s.length && k == s[f].from) {
          if (g)
            break e;
          let C = s[f];
          if (!u)
            for (let M = C.to, R = f + 1; ; ) {
              if (M == t)
                break e;
              if (R < s.length && s[R].from == M)
                M = s[R++].to;
              else {
                if (ke[M] == l)
                  break e;
                break;
              }
            }
          if (f++, y)
            y.push(C);
          else {
            C.from > a && o.push(new si(a, C.from, b));
            let M = C.direction == Ki != !(b % 2);
            ll(n, M ? i + 1 : i, r, C.inner, C.from, C.to, o), a = C.to;
          }
          k = C.to;
        } else {
          if (k == t || (u ? ke[k] != l : ke[k] == l))
            break;
          k++;
        }
      y ? ol(n, a, k, i + 1, r, y, o) : a < k && o.push(new si(a, k, b)), a = k;
    }
  else
    for (let a = t, f = s.length; a > e; ) {
      let u = !0, g = !1;
      if (!f || a > s[f - 1].to) {
        let C = ke[a - 1];
        C != l && (u = !1, g = C == 16);
      }
      let y = !u && l == 1 ? [] : null, b = u ? i : i + 1, k = a;
      e: for (; ; )
        if (f && k == s[f - 1].to) {
          if (g)
            break e;
          let C = s[--f];
          if (!u)
            for (let M = C.from, R = f; ; ) {
              if (M == e)
                break e;
              if (R && s[R - 1].to == M)
                M = s[--R].from;
              else {
                if (ke[M - 1] == l)
                  break e;
                break;
              }
            }
          if (y)
            y.push(C);
          else {
            C.to < a && o.push(new si(C.to, a, b));
            let M = C.direction == Ki != !(b % 2);
            ll(n, M ? i + 1 : i, r, C.inner, C.from, C.to, o), a = C.from;
          }
          k = C.from;
        } else {
          if (k == e || (u ? ke[k - 1] != l : ke[k - 1] == l))
            break;
          k--;
        }
      y ? ol(n, k, a, i + 1, r, y, o) : k < a && o.push(new si(k, a, b)), a = k;
    }
}
function ll(n, e, t, i, r, s, o) {
  let l = e % 2 ? 2 : 1;
  fm(n, r, s, i, l), um(n, r, s, i, l), dm(r, s, i, l), ol(n, r, s, e, t, i, o);
}
function pm(n, e, t) {
  if (!n)
    return [new si(0, 0, e == _l ? 1 : 0)];
  if (e == Ki && !t.length && !cm.test(n))
    return Jf(n.length);
  if (t.length)
    for (; n.length > ke.length; )
      ke[ke.length] = 256;
  let i = [], r = e == Ki ? 0 : 1;
  return ll(n, r, r, t, 0, n.length, i), i;
}
function Jf(n) {
  return [new si(0, n, 0)];
}
let eu = "";
function gm(n, e, t, i, r) {
  var s;
  let o = i.head - n.from, l = si.find(e, o, (s = i.bidiLevel) !== null && s !== void 0 ? s : -1, i.assoc), a = e[l], f = a.side(r, t);
  if (o == f) {
    let y = l += r ? 1 : -1;
    if (y < 0 || y >= e.length)
      return null;
    a = e[l = y], o = a.side(!r, t), f = a.side(r, t);
  }
  let u = We(n.text, o, a.forward(r, t));
  (u < a.from || u > a.to) && (u = f), eu = n.text.slice(Math.min(o, u), Math.max(o, u));
  let g = l == (r ? e.length - 1 : 0) ? null : e[l + (r ? 1 : -1)];
  return g && u == f && g.level + (r ? 0 : 1) < a.level ? E.cursor(g.side(!r, t) + n.from, g.forward(r, t) ? 1 : -1, g.level) : E.cursor(u + n.from, a.forward(r, t) ? -1 : 1, a.level);
}
function mm(n, e, t) {
  for (let i = e; i < t; i++) {
    let r = Gf(n.charCodeAt(i));
    if (r == 1)
      return Ki;
    if (r == 2 || r == 4)
      return _l;
  }
  return Ki;
}
const tu = /* @__PURE__ */ U.define(), iu = /* @__PURE__ */ U.define(), nu = /* @__PURE__ */ U.define(), ru = /* @__PURE__ */ U.define(), al = /* @__PURE__ */ U.define(), su = /* @__PURE__ */ U.define(), ou = /* @__PURE__ */ U.define(), jl = /* @__PURE__ */ U.define(), Ul = /* @__PURE__ */ U.define(), lu = /* @__PURE__ */ U.define({
  combine: (n) => n.some((e) => e)
}), au = /* @__PURE__ */ U.define({
  combine: (n) => n.some((e) => e)
}), hu = /* @__PURE__ */ U.define();
class un {
  constructor(e, t = "nearest", i = "nearest", r = 5, s = 5, o = !1) {
    this.range = e, this.y = t, this.x = i, this.yMargin = r, this.xMargin = s, this.isSnapshot = o;
  }
  map(e) {
    return e.empty ? this : new un(this.range.map(e), this.y, this.x, this.yMargin, this.xMargin, this.isSnapshot);
  }
  clip(e) {
    return this.range.to <= e.doc.length ? this : new un(E.cursor(e.doc.length), this.y, this.x, this.yMargin, this.xMargin, this.isSnapshot);
  }
}
const Cr = /* @__PURE__ */ ne.define({ map: (n, e) => n.map(e) }), cu = /* @__PURE__ */ ne.define();
function ft(n, e, t) {
  let i = n.facet(ru);
  i.length ? i[0](e) : window.onerror && window.onerror(String(e), t, void 0, void 0, e) || (t ? console.error(t + ":", e) : console.error(e));
}
const ri = /* @__PURE__ */ U.define({ combine: (n) => n.length ? n[0] : !0 });
let vm = 0;
const sn = /* @__PURE__ */ U.define({
  combine(n) {
    return n.filter((e, t) => {
      for (let i = 0; i < t; i++)
        if (n[i].plugin == e.plugin)
          return !1;
      return !0;
    });
  }
});
class De {
  constructor(e, t, i, r, s) {
    this.id = e, this.create = t, this.domEventHandlers = i, this.domEventObservers = r, this.baseExtensions = s(this), this.extension = this.baseExtensions.concat(sn.of({ plugin: this, arg: void 0 }));
  }
  /**
  Create an extension for this plugin with the given argument.
  */
  of(e) {
    return this.baseExtensions.concat(sn.of({ plugin: this, arg: e }));
  }
  /**
  Define a plugin from a constructor function that creates the
  plugin's value, given an editor view.
  */
  static define(e, t) {
    const { eventHandlers: i, eventObservers: r, provide: s, decorations: o } = t || {};
    return new De(vm++, e, i, r, (l) => {
      let a = [];
      return o && a.push(Es.of((f) => {
        let u = f.plugin(l);
        return u ? o(u) : G.none;
      })), s && a.push(s(l)), a;
    });
  }
  /**
  Create a plugin for a class whose constructor takes a single
  editor view as argument.
  */
  static fromClass(e, t) {
    return De.define((i, r) => new e(i, r), t);
  }
}
class co {
  constructor(e) {
    this.spec = e, this.mustUpdate = null, this.value = null;
  }
  get plugin() {
    return this.spec && this.spec.plugin;
  }
  update(e) {
    if (this.value) {
      if (this.mustUpdate) {
        let t = this.mustUpdate;
        if (this.mustUpdate = null, this.value.update)
          try {
            this.value.update(t);
          } catch (i) {
            if (ft(t.state, i, "CodeMirror plugin crashed"), this.value.destroy)
              try {
                this.value.destroy();
              } catch {
              }
            this.deactivate();
          }
      }
    } else if (this.spec)
      try {
        this.value = this.spec.plugin.create(e, this.spec.arg);
      } catch (t) {
        ft(e.state, t, "CodeMirror plugin crashed"), this.deactivate();
      }
    return this;
  }
  destroy(e) {
    var t;
    if (!((t = this.value) === null || t === void 0) && t.destroy)
      try {
        this.value.destroy();
      } catch (i) {
        ft(e.state, i, "CodeMirror plugin crashed");
      }
  }
  deactivate() {
    this.spec = this.value = null;
  }
}
const fu = /* @__PURE__ */ U.define(), Xl = /* @__PURE__ */ U.define(), Es = /* @__PURE__ */ U.define(), uu = /* @__PURE__ */ U.define(), Yl = /* @__PURE__ */ U.define(), ur = /* @__PURE__ */ U.define(), du = /* @__PURE__ */ U.define();
function Ah(n, e) {
  let t = n.state.facet(du);
  if (!t.length)
    return t;
  let i = t.map((s) => s instanceof Function ? s(n) : s), r = [];
  return ce.spans(i, e.from, e.to, {
    point() {
    },
    span(s, o, l, a) {
      let f = s - e.from, u = o - e.from, g = r;
      for (let y = l.length - 1; y >= 0; y--, a--) {
        let b = l[y].spec.bidiIsolate, k;
        if (b == null && (b = mm(e.text, f, u)), a > 0 && g.length && (k = g[g.length - 1]).to == f && k.direction == b)
          k.to = u, g = k.inner;
        else {
          let C = { from: f, to: u, direction: b, inner: [] };
          g.push(C), g = C.inner;
        }
      }
    }
  }), r;
}
const pu = /* @__PURE__ */ U.define();
function Gl(n) {
  let e = 0, t = 0, i = 0, r = 0;
  for (let s of n.state.facet(pu)) {
    let o = s(n);
    o && (o.left != null && (e = Math.max(e, o.left)), o.right != null && (t = Math.max(t, o.right)), o.top != null && (i = Math.max(i, o.top)), o.bottom != null && (r = Math.max(r, o.bottom)));
  }
  return { left: e, right: t, top: i, bottom: r };
}
const Wn = /* @__PURE__ */ U.define();
class Ot {
  constructor(e, t, i, r) {
    this.fromA = e, this.toA = t, this.fromB = i, this.toB = r;
  }
  join(e) {
    return new Ot(Math.min(this.fromA, e.fromA), Math.max(this.toA, e.toA), Math.min(this.fromB, e.fromB), Math.max(this.toB, e.toB));
  }
  addToSet(e) {
    let t = e.length, i = this;
    for (; t > 0; t--) {
      let r = e[t - 1];
      if (!(r.fromA > i.toA)) {
        if (r.toA < i.fromA)
          break;
        i = i.join(r), e.splice(t - 1, 1);
      }
    }
    return e.splice(t, 0, i), e;
  }
  // Extend a set to cover all the content in `ranges`, which is a
  // flat array with each pair of numbers representing fromB/toB
  // positions. These pairs are generated in unchanged ranges, so the
  // offset between doc A and doc B is the same for their start and
  // end points.
  static extendWithRanges(e, t) {
    if (t.length == 0)
      return e;
    let i = [];
    for (let r = 0, s = 0, o = 0; ; ) {
      let l = r < e.length ? e[r].fromB : 1e9, a = s < t.length ? t[s] : 1e9, f = Math.min(l, a);
      if (f == 1e9)
        break;
      let u = f + o, g = f, y = u;
      for (; ; )
        if (s < t.length && t[s] <= g) {
          let b = t[s + 1];
          s += 2, g = Math.max(g, b);
          for (let k = r; k < e.length && e[k].fromB <= g; k++)
            o = e[k].toA - e[k].toB;
          y = Math.max(y, b + o);
        } else if (r < e.length && e[r].fromB <= g) {
          let b = e[r++];
          g = Math.max(g, b.toB), y = Math.max(y, b.toA), o = b.toA - b.toB;
        } else
          break;
      i.push(new Ot(u, y, f, g));
    }
    return i;
  }
}
class os {
  constructor(e, t, i) {
    this.view = e, this.state = t, this.transactions = i, this.flags = 0, this.startState = e.state, this.changes = Fe.empty(this.startState.doc.length);
    for (let s of i)
      this.changes = this.changes.compose(s.changes);
    let r = [];
    this.changes.iterChangedRanges((s, o, l, a) => r.push(new Ot(s, o, l, a))), this.changedRanges = r;
  }
  /**
  @internal
  */
  static create(e, t, i) {
    return new os(e, t, i);
  }
  /**
  Tells you whether the [viewport](https://codemirror.net/6/docs/ref/#view.EditorView.viewport) or
  [visible ranges](https://codemirror.net/6/docs/ref/#view.EditorView.visibleRanges) changed in this
  update.
  */
  get viewportChanged() {
    return (this.flags & 4) > 0;
  }
  /**
  Returns true when
  [`viewportChanged`](https://codemirror.net/6/docs/ref/#view.ViewUpdate.viewportChanged) is true
  and the viewport change is not just the result of mapping it in
  response to document changes.
  */
  get viewportMoved() {
    return (this.flags & 8) > 0;
  }
  /**
  Indicates whether the height of a block element in the editor
  changed in this update.
  */
  get heightChanged() {
    return (this.flags & 2) > 0;
  }
  /**
  Returns true when the document was modified or the size of the
  editor, or elements within the editor, changed.
  */
  get geometryChanged() {
    return this.docChanged || (this.flags & 18) > 0;
  }
  /**
  True when this update indicates a focus change.
  */
  get focusChanged() {
    return (this.flags & 1) > 0;
  }
  /**
  Whether the document changed in this update.
  */
  get docChanged() {
    return !this.changes.empty;
  }
  /**
  Whether the selection was explicitly set in this update.
  */
  get selectionSet() {
    return this.transactions.some((e) => e.selection);
  }
  /**
  @internal
  */
  get empty() {
    return this.flags == 0 && this.transactions.length == 0;
  }
}
const ym = [];
class Ne {
  constructor(e, t, i = 0) {
    this.dom = e, this.length = t, this.flags = i, this.parent = null, e.cmTile = this;
  }
  get breakAfter() {
    return this.flags & 1;
  }
  get children() {
    return ym;
  }
  isWidget() {
    return !1;
  }
  get isHidden() {
    return !1;
  }
  isComposite() {
    return !1;
  }
  isLine() {
    return !1;
  }
  isText() {
    return !1;
  }
  isBlock() {
    return !1;
  }
  get domAttrs() {
    return null;
  }
  sync(e) {
    if (this.flags |= 2, this.flags & 4) {
      this.flags &= -5;
      let t = this.domAttrs;
      t && Jg(this.dom, t);
    }
  }
  toString() {
    return this.constructor.name + (this.children.length ? `(${this.children})` : "") + (this.breakAfter ? "#" : "");
  }
  destroy() {
    this.parent = null;
  }
  setDOM(e) {
    this.dom = e, e.cmTile = this;
  }
  get posAtStart() {
    return this.parent ? this.parent.posBefore(this) : 0;
  }
  get posAtEnd() {
    return this.posAtStart + this.length;
  }
  posBefore(e, t = this.posAtStart) {
    let i = t;
    for (let r of this.children) {
      if (r == e)
        return i;
      i += r.length + r.breakAfter;
    }
    throw new RangeError("Invalid child in posBefore");
  }
  posAfter(e) {
    return this.posBefore(e) + e.length;
  }
  covers(e) {
    return !0;
  }
  coordsIn(e, t) {
    return null;
  }
  domPosFor(e, t) {
    let i = Ci(this.dom), r = this.length ? e > 0 : t > 0;
    return new Nt(this.parent.dom, i + (r ? 1 : 0), e == 0 || e == this.length);
  }
  markDirty(e) {
    this.flags &= -3, e && (this.flags |= 4), this.parent && this.parent.flags & 2 && this.parent.markDirty(!1);
  }
  get overrideDOMText() {
    return null;
  }
  get root() {
    for (let e = this; e; e = e.parent)
      if (e instanceof Ns)
        return e;
    return null;
  }
  static get(e) {
    return e.cmTile;
  }
}
class Is extends Ne {
  constructor(e) {
    super(e, 0), this._children = [];
  }
  isComposite() {
    return !0;
  }
  get children() {
    return this._children;
  }
  get lastChild() {
    return this.children.length ? this.children[this.children.length - 1] : null;
  }
  append(e) {
    this.children.push(e), e.parent = this;
  }
  sync(e) {
    if (this.flags & 2)
      return;
    super.sync(e);
    let t = this.dom, i = null, r, s = e?.node == t ? e : null, o = 0;
    for (let l of this.children) {
      if (l.sync(e), o += l.length + l.breakAfter, r = i ? i.nextSibling : t.firstChild, s && r != l.dom && (s.written = !0), l.dom.parentNode == t)
        for (; r && r != l.dom; )
          r = Mh(r);
      else
        t.insertBefore(l.dom, r);
      i = l.dom;
    }
    for (r = i ? i.nextSibling : t.firstChild, s && r && (s.written = !0); r; )
      r = Mh(r);
    this.length = o;
  }
}
function Mh(n) {
  let e = n.nextSibling;
  return n.parentNode.removeChild(n), e;
}
class Ns extends Is {
  constructor(e, t) {
    super(t), this.view = e;
  }
  owns(e) {
    for (; e; e = e.parent)
      if (e == this)
        return !0;
    return !1;
  }
  isBlock() {
    return !0;
  }
  nearest(e) {
    for (; ; ) {
      if (!e)
        return null;
      let t = Ne.get(e);
      if (t && this.owns(t))
        return t;
      e = e.parentNode;
    }
  }
  blockTiles(e) {
    for (let t = [], i = this, r = 0, s = 0; ; )
      if (r == i.children.length) {
        if (!t.length)
          return;
        i = i.parent, i.breakAfter && s++, r = t.pop();
      } else {
        let o = i.children[r++];
        if (o instanceof bi)
          t.push(r), i = o, r = 0;
        else {
          let l = s + o.length, a = e(o, s);
          if (a !== void 0)
            return a;
          s = l + o.breakAfter;
        }
      }
  }
  // Find the block at the given position. If side < -1, make sure to
  // stay before block widgets at that position, if side > 1, after
  // such widgets (used for selection drawing, which needs to be able
  // to get coordinates for positions that aren't valid cursor positions).
  resolveBlock(e, t) {
    let i, r = -1, s, o = -1;
    if (this.blockTiles((l, a) => {
      let f = a + l.length;
      if (e >= a && e <= f) {
        if (l.isWidget() && t >= -1 && t <= 1) {
          if (l.flags & 32)
            return !0;
          l.flags & 16 && (i = void 0);
        }
        (a < e || e == f && (t < -1 ? l.length : l.covers(1))) && (!i || !l.isWidget() && i.isWidget()) && (i = l, r = e - a), (f > e || e == a && (t > 1 ? l.length : l.covers(-1))) && (!s || !l.isWidget() && s.isWidget()) && (s = l, o = e - a);
      }
    }), !i && !s)
      throw new Error("No tile at position " + e);
    return i && t < 0 || !s ? { tile: i, offset: r } : { tile: s, offset: o };
  }
}
class bi extends Is {
  constructor(e, t) {
    super(e), this.wrapper = t;
  }
  isBlock() {
    return !0;
  }
  covers(e) {
    return this.children.length ? e < 0 ? this.children[0].covers(-1) : this.lastChild.covers(1) : !1;
  }
  get domAttrs() {
    return this.wrapper.attributes;
  }
  static of(e, t) {
    let i = new bi(t || document.createElement(e.tagName), e);
    return t || (i.flags |= 4), i;
  }
}
class bn extends Is {
  constructor(e, t) {
    super(e), this.attrs = t;
  }
  isLine() {
    return !0;
  }
  static start(e, t, i) {
    let r = new bn(t || document.createElement("div"), e);
    return (!t || !i) && (r.flags |= 4), r;
  }
  get domAttrs() {
    return this.attrs;
  }
  // Find the tile associated with a given position in this line.
  resolveInline(e, t, i) {
    let r = null, s = -1, o = null, l = -1;
    function a(u, g) {
      for (let y = 0, b = 0; y < u.children.length && b <= g; y++) {
        let k = u.children[y], C = b + k.length;
        C >= g && (k.isComposite() ? a(k, g - b) : (!o || o.isHidden && (t > 0 || i && xm(o, k))) && (C > g || k.flags & 32) ? (o = k, l = g - b) : (b < g || k.flags & 16 && !k.isHidden) && (r = k, s = g - b)), b = C;
      }
    }
    a(this, e);
    let f = (t < 0 ? r : o) || r || o;
    return f ? { tile: f, offset: f == r ? s : l } : null;
  }
  coordsIn(e, t) {
    let i = this.resolveInline(e, t, !0);
    return i ? i.tile.coordsIn(Math.max(0, i.offset), t) : bm(this);
  }
  domIn(e, t) {
    let i = this.resolveInline(e, t);
    if (i) {
      let { tile: r, offset: s } = i;
      if (this.dom.contains(r.dom))
        return r.isText() ? new Nt(r.dom, Math.min(r.dom.nodeValue.length, s)) : r.domPosFor(s, r.flags & 16 ? 1 : r.flags & 32 ? -1 : t);
      let o = i.tile.parent, l = !1;
      for (let a of o.children) {
        if (l)
          return new Nt(a.dom, 0);
        a == i.tile && (l = !0);
      }
    }
    return new Nt(this.dom, 0);
  }
}
function bm(n) {
  let e = n.dom.lastChild;
  if (!e)
    return n.dom.getBoundingClientRect();
  let t = jr(e);
  return t[t.length - 1] || null;
}
function xm(n, e) {
  let t = n.coordsIn(0, 1), i = e.coordsIn(0, 1);
  return t && i && i.top < t.bottom;
}
class ct extends Is {
  constructor(e, t) {
    super(e), this.mark = t;
  }
  get domAttrs() {
    return this.mark.attrs;
  }
  static of(e, t) {
    let i = new ct(t || document.createElement(e.tagName), e);
    return t || (i.flags |= 4), i;
  }
}
class Hi extends Ne {
  constructor(e, t) {
    super(e, t.length), this.text = t;
  }
  sync(e) {
    this.flags & 2 || (super.sync(e), this.dom.nodeValue != this.text && (e && e.node == this.dom && (e.written = !0), this.dom.nodeValue = this.text));
  }
  isText() {
    return !0;
  }
  toString() {
    return JSON.stringify(this.text);
  }
  coordsIn(e, t) {
    let i = this.dom.nodeValue.length;
    e > i && (e = i);
    let r = e, s = e, o = 0;
    e == 0 && t < 0 || e == i && t >= 0 ? j.chrome || j.gecko || (e ? (r--, o = 1) : s < i && (s++, o = -1)) : t < 0 ? r-- : s < i && s++;
    let l = Jn(this.dom, r, s).getClientRects();
    if (!l.length)
      return null;
    let a = l[(o ? o < 0 : t >= 0) ? 0 : l.length - 1];
    return j.safari && !o && a.width == 0 && (a = Array.prototype.find.call(l, (f) => f.width) || a), o ? ss(a, o < 0) : a || null;
  }
  static of(e, t) {
    let i = new Hi(t || document.createTextNode(e), e);
    return t || (i.flags |= 2), i;
  }
}
class _i extends Ne {
  constructor(e, t, i, r) {
    super(e, t, r), this.widget = i;
  }
  isWidget() {
    return !0;
  }
  get isHidden() {
    return this.widget.isHidden;
  }
  covers(e) {
    return this.flags & 48 ? !1 : (this.flags & (e < 0 ? 64 : 128)) > 0;
  }
  coordsIn(e, t) {
    return this.coordsInWidget(e, t, !1);
  }
  coordsInWidget(e, t, i) {
    let r = this.widget.coordsAt(this.dom, e, t);
    if (r)
      return r;
    if (i)
      return ss(this.dom.getBoundingClientRect(), this.length ? e == 0 : t <= 0);
    {
      let s = this.dom.getClientRects(), o = null;
      if (!s.length)
        return null;
      let l = this.flags & 16 ? !0 : this.flags & 32 ? !1 : e > 0;
      for (let a = l ? s.length - 1 : 0; o = s[a], !(e > 0 ? a == 0 : a == s.length - 1 || o.top < o.bottom); a += l ? -1 : 1)
        ;
      return ss(o, !l);
    }
  }
  get overrideDOMText() {
    if (!this.length)
      return ge.empty;
    let { root: e } = this;
    if (!e)
      return ge.empty;
    let t = this.posAtStart;
    return e.view.state.doc.slice(t, t + this.length);
  }
  destroy() {
    super.destroy(), this.widget.destroy(this.dom);
  }
  static of(e, t, i, r, s) {
    return s || (s = e.toDOM(t), e.editable || (s.contentEditable = "false")), new _i(s, i, e, r);
  }
}
class ls extends Ne {
  constructor(e) {
    let t = document.createElement("img");
    t.className = "cm-widgetBuffer", t.setAttribute("aria-hidden", "true"), super(t, 0, e);
  }
  get isHidden() {
    return !0;
  }
  get overrideDOMText() {
    return ge.empty;
  }
  coordsIn(e) {
    return this.dom.getBoundingClientRect();
  }
}
class km {
  constructor(e) {
    this.index = 0, this.beforeBreak = !1, this.parents = [], this.tile = e;
  }
  // Advance by the given distance. If side is -1, stop leaving or
  // entering tiles, or skipping zero-length tiles, once the distance
  // has been traversed. When side is 1, leave, enter, or skip
  // everything at the end position.
  advance(e, t, i) {
    let { tile: r, index: s, beforeBreak: o, parents: l } = this;
    for (; e || t > 0; )
      if (r.isComposite())
        if (o) {
          if (!e)
            break;
          i && i.break(), e--, o = !1;
        } else if (s == r.children.length) {
          if (!e && !l.length)
            break;
          i && i.leave(r), o = !!r.breakAfter, { tile: r, index: s } = l.pop(), s++;
        } else {
          let a = r.children[s], f = a.breakAfter;
          (t > 0 ? a.length <= e : a.length < e) && (!i || i.skip(a, 0, a.length) !== !1 || !a.isComposite) ? (o = !!f, s++, e -= a.length) : (l.push({ tile: r, index: s }), r = a, s = 0, i && a.isComposite() && i.enter(a));
        }
      else if (s == r.length)
        o = !!r.breakAfter, { tile: r, index: s } = l.pop(), s++;
      else if (e) {
        let a = Math.min(e, r.length - s);
        i && i.skip(r, s, s + a), e -= a, s += a;
      } else
        break;
    return this.tile = r, this.index = s, this.beforeBreak = o, this;
  }
  get root() {
    return this.parents.length ? this.parents[0].tile : this.tile;
  }
}
class wm {
  constructor(e, t, i, r) {
    this.from = e, this.to = t, this.wrapper = i, this.rank = r;
  }
}
class Sm {
  constructor(e, t, i) {
    this.cache = e, this.root = t, this.blockWrappers = i, this.curLine = null, this.lastBlock = null, this.afterWidget = null, this.pos = 0, this.wrappers = [], this.wrapperPos = 0;
  }
  addText(e, t, i, r) {
    var s;
    this.flushBuffer();
    let o = this.ensureMarks(t, i), l = o.lastChild;
    if (l && l.isText() && !(l.flags & 8)) {
      this.cache.reused.set(
        l,
        2
        /* Reused.DOM */
      );
      let a = o.children[o.children.length - 1] = new Hi(l.dom, l.text + e);
      a.parent = o;
    } else
      o.append(r || Hi.of(e, (s = this.cache.find(Hi)) === null || s === void 0 ? void 0 : s.dom));
    this.pos += e.length, this.afterWidget = null;
  }
  addComposition(e, t) {
    let i = this.curLine;
    i.dom != t.line.dom && (i.setDOM(this.cache.reused.has(t.line) ? fo(t.line.dom) : t.line.dom), this.cache.reused.set(
      t.line,
      2
      /* Reused.DOM */
    ));
    let r = i;
    for (let l = t.marks.length - 1; l >= 0; l--) {
      let a = t.marks[l], f = r.lastChild;
      if (f instanceof ct && f.mark.eq(a.mark))
        f.dom != a.dom && f.setDOM(fo(a.dom)), r = f;
      else {
        if (this.cache.reused.get(a)) {
          let g = Ne.get(a.dom);
          g && g.setDOM(fo(a.dom));
        }
        let u = ct.of(a.mark, a.dom);
        r.append(u), r = u;
      }
      this.cache.reused.set(
        a,
        2
        /* Reused.DOM */
      );
    }
    let s = Ne.get(e.text);
    s && this.cache.reused.set(
      s,
      2
      /* Reused.DOM */
    );
    let o = new Hi(e.text, e.text.nodeValue);
    o.flags |= 8, r.append(o);
  }
  addInlineWidget(e, t, i) {
    let r = this.afterWidget && e.flags & 48 && (this.afterWidget.flags & 48) == (e.flags & 48);
    r || this.flushBuffer();
    let s = this.ensureMarks(t, i);
    !r && !(e.flags & 16) && s.append(this.getBuffer(1)), s.append(e), this.pos += e.length, this.afterWidget = e;
  }
  addMark(e, t, i) {
    this.flushBuffer(), this.ensureMarks(t, i).append(e), this.pos += e.length, this.afterWidget = null;
  }
  addBlockWidget(e) {
    this.getBlockPos().append(e), this.pos += e.length, this.lastBlock = e, this.endLine();
  }
  continueWidget(e) {
    let t = this.afterWidget || this.lastBlock;
    t.length += e, this.pos += e;
  }
  addLineStart(e, t) {
    var i;
    e || (e = gu);
    let r = bn.start(e, t || ((i = this.cache.find(bn)) === null || i === void 0 ? void 0 : i.dom), !!t);
    this.getBlockPos().append(this.lastBlock = this.curLine = r);
  }
  addLine(e) {
    this.getBlockPos().append(e), this.pos += e.length, this.lastBlock = e, this.endLine();
  }
  addBreak() {
    this.lastBlock.flags |= 1, this.endLine(), this.pos++;
  }
  addLineStartIfNotCovered(e) {
    this.blockPosCovered() || this.addLineStart(e);
  }
  ensureLine(e) {
    this.curLine || this.addLineStart(e);
  }
  ensureMarks(e, t) {
    var i;
    let r = this.curLine;
    for (let s = e.length - 1; s >= 0; s--) {
      let o = e[s], l;
      if (t > 0 && (l = r.lastChild) && l instanceof ct && l.mark.eq(o))
        r = l, t--;
      else {
        let a = ct.of(o, (i = this.cache.find(ct, (f) => f.mark.eq(o))) === null || i === void 0 ? void 0 : i.dom);
        r.append(a), r = a, t = 0;
      }
    }
    return r;
  }
  endLine() {
    if (this.curLine) {
      this.flushBuffer();
      let e = this.curLine.lastChild;
      (!e || !Th(this.curLine, !1) || e.dom.nodeName != "BR" && e.isWidget() && !(j.ios && Th(this.curLine, !0))) && this.curLine.append(this.cache.findWidget(
        uo,
        0,
        32
        /* TileFlag.After */
      ) || new _i(
        uo.toDOM(),
        0,
        uo,
        32
        /* TileFlag.After */
      )), this.curLine = this.afterWidget = null;
    }
  }
  updateBlockWrappers() {
    this.wrapperPos > this.pos + 1e4 && (this.blockWrappers.goto(this.pos), this.wrappers.length = 0);
    for (let e = this.wrappers.length - 1; e >= 0; e--)
      this.wrappers[e].to < this.pos && this.wrappers.splice(e, 1);
    for (let e = this.blockWrappers; e.value && e.from <= this.pos; e.next())
      if (e.to >= this.pos) {
        let t = new wm(e.from, e.to, e.value, e.rank), i = this.wrappers.length;
        for (; i > 0 && (this.wrappers[i - 1].rank - t.rank || this.wrappers[i - 1].to - t.to) < 0; )
          i--;
        this.wrappers.splice(i, 0, t);
      }
    this.wrapperPos = this.pos;
  }
  getBlockPos() {
    var e;
    this.updateBlockWrappers();
    let t = this.root;
    for (let i of this.wrappers) {
      let r = t.lastChild;
      if (i.from < this.pos && r instanceof bi && r.wrapper.eq(i.wrapper))
        t = r;
      else {
        let s = bi.of(i.wrapper, (e = this.cache.find(bi, (o) => o.wrapper.eq(i.wrapper))) === null || e === void 0 ? void 0 : e.dom);
        t.append(s), t = s;
      }
    }
    return t;
  }
  blockPosCovered() {
    let e = this.lastBlock;
    return e != null && !e.breakAfter && (!e.isWidget() || (e.flags & 160) > 0);
  }
  getBuffer(e) {
    let t = 2 | (e < 0 ? 16 : 32), i = this.cache.find(
      ls,
      void 0,
      1
      /* Reused.Full */
    );
    return i && (i.flags = t), i || new ls(t);
  }
  flushBuffer() {
    this.afterWidget && !(this.afterWidget.flags & 32) && (this.afterWidget.parent.append(this.getBuffer(-1)), this.afterWidget = null);
  }
}
class Cm {
  constructor(e) {
    this.skipCount = 0, this.text = "", this.textOff = 0, this.cursor = e.iter();
  }
  skip(e) {
    this.textOff + e <= this.text.length ? this.textOff += e : (this.skipCount += e - (this.text.length - this.textOff), this.text = "", this.textOff = 0);
  }
  next(e) {
    if (this.textOff == this.text.length) {
      let { value: r, lineBreak: s, done: o } = this.cursor.next(this.skipCount);
      if (this.skipCount = 0, o)
        throw new Error("Ran out of text content when drawing inline views");
      this.text = r;
      let l = this.textOff = Math.min(e, r.length);
      return s ? null : r.slice(0, l);
    }
    let t = Math.min(this.text.length, this.textOff + e), i = this.text.slice(this.textOff, t);
    return this.textOff = t, i;
  }
}
const as = [_i, bn, Hi, ct, ls, bi, Ns];
for (let n = 0; n < as.length; n++)
  as[n].bucket = n;
class Om {
  constructor(e) {
    this.view = e, this.buckets = as.map(() => []), this.index = as.map(() => 0), this.reused = /* @__PURE__ */ new Map();
  }
  // Put a tile in the cache.
  add(e) {
    let t = e.constructor.bucket, i = this.buckets[t];
    i.length < 6 ? i.push(e) : i[
      this.index[t] = (this.index[t] + 1) % 6
      /* C.Bucket */
    ] = e;
  }
  find(e, t, i = 2) {
    let r = e.bucket, s = this.buckets[r], o = this.index[r];
    for (let l = s.length - 1; l >= 0; l--) {
      let a = (l + o) % s.length, f = s[a];
      if ((!t || t(f)) && !this.reused.has(f))
        return s.splice(a, 1), a < o && this.index[r]--, this.reused.set(f, i), f;
    }
    return null;
  }
  findWidget(e, t, i) {
    let r = this.buckets[0];
    if (r.length)
      for (let s = 0, o = 0; ; s++) {
        if (s == r.length) {
          if (o)
            return null;
          o = 1, s = 0;
        }
        let l = r[s];
        if (!this.reused.has(l) && (o == 0 ? l.widget.compare(e) : l.widget.constructor == e.constructor && e.updateDOM(l.dom, this.view)))
          return r.splice(s, 1), s < this.index[0] && this.index[0]--, l.widget == e && l.length == t && (l.flags & 497) == i ? (this.reused.set(
            l,
            1
            /* Reused.Full */
          ), l) : (this.reused.set(
            l,
            2
            /* Reused.DOM */
          ), new _i(l.dom, t, e, l.flags & -498 | i));
      }
  }
  reuse(e) {
    return this.reused.set(
      e,
      1
      /* Reused.Full */
    ), e;
  }
  maybeReuse(e, t = 2) {
    if (!this.reused.has(e))
      return this.reused.set(e, t), e.dom;
  }
  clear() {
    for (let e = 0; e < this.buckets.length; e++)
      this.buckets[e].length = this.index[e] = 0;
  }
}
class Am {
  constructor(e, t, i, r, s) {
    this.view = e, this.decorations = r, this.disallowBlockEffectsFor = s, this.openWidget = !1, this.openMarks = 0, this.cache = new Om(e), this.text = new Cm(e.state.doc), this.builder = new Sm(this.cache, new Ns(e, e.contentDOM), ce.iter(i)), this.cache.reused.set(
      t,
      2
      /* Reused.DOM */
    ), this.old = new km(t), this.reuseWalker = {
      skip: (o, l, a) => {
        if (this.cache.add(o), o.isComposite())
          return !1;
      },
      enter: (o) => this.cache.add(o),
      leave: () => {
      },
      break: () => {
      }
    };
  }
  run(e, t) {
    let i = t && this.getCompositionContext(t.text);
    for (let r = 0, s = 0, o = 0; ; ) {
      let l = o < e.length ? e[o++] : null, a = l ? l.fromA : this.old.root.length;
      if (a > r) {
        let f = a - r;
        this.preserve(f, !o, !l), r = a, s += f;
      }
      if (!l)
        break;
      t && l.fromA <= t.range.fromA && l.toA >= t.range.toA ? (this.forward(l.fromA, t.range.fromA, t.range.fromA < t.range.toA ? 1 : -1), this.emit(s, t.range.fromB), this.cache.clear(), this.builder.addComposition(t, i), this.text.skip(t.range.toB - t.range.fromB), this.forward(t.range.fromA, l.toA), this.emit(t.range.toB, l.toB)) : (this.forward(l.fromA, l.toA), this.emit(s, l.toB)), s = l.toB, r = l.toA;
    }
    return this.builder.curLine && this.builder.endLine(), this.builder.root;
  }
  preserve(e, t, i) {
    let r = Pm(this.old), s = this.openMarks;
    this.old.advance(e, i ? 1 : -1, {
      skip: (o, l, a) => {
        if (o.isWidget())
          if (this.openWidget)
            this.builder.continueWidget(a - l);
          else {
            let f = a > 0 || l < o.length ? _i.of(o.widget, this.view, a - l, o.flags & 496, this.cache.maybeReuse(o)) : this.cache.reuse(o);
            f.flags & 256 ? (f.flags &= -2, this.builder.addBlockWidget(f)) : (this.builder.ensureLine(null), this.builder.addInlineWidget(f, r, s), s = r.length);
          }
        else if (o.isText())
          this.builder.ensureLine(null), !l && a == o.length ? this.builder.addText(o.text, r, s, this.cache.reuse(o)) : (this.cache.add(o), this.builder.addText(o.text.slice(l, a), r, s)), s = r.length;
        else if (o.isLine())
          o.flags &= -2, this.cache.reused.set(
            o,
            1
            /* Reused.Full */
          ), this.builder.addLine(o);
        else if (o instanceof ls)
          this.cache.add(o);
        else if (o instanceof ct)
          this.builder.ensureLine(null), this.builder.addMark(o, r, s), this.cache.reused.set(
            o,
            1
            /* Reused.Full */
          ), s = r.length;
        else
          return !1;
        this.openWidget = !1;
      },
      enter: (o) => {
        o.isLine() ? this.builder.addLineStart(o.attrs, this.cache.maybeReuse(o)) : (this.cache.add(o), o instanceof ct && r.unshift(o.mark)), this.openWidget = !1;
      },
      leave: (o) => {
        o.isLine() ? r.length && (r.length = s = 0) : o instanceof ct && (r.shift(), s = Math.min(s, r.length));
      },
      break: () => {
        this.builder.addBreak(), this.openWidget = !1;
      }
    }), this.text.skip(e);
  }
  emit(e, t) {
    let i = null, r = this.builder, s = 0, o = ce.spans(this.decorations, e, t, {
      point: (l, a, f, u, g, y) => {
        if (f instanceof qi) {
          if (this.disallowBlockEffectsFor[y]) {
            if (f.block)
              throw new RangeError("Block decorations may not be specified via plugins");
            if (a > this.view.state.doc.lineAt(l).to)
              throw new RangeError("Decorations that replace line breaks may not be specified via plugins");
          }
          if (s = u.length, g > u.length)
            r.continueWidget(a - l);
          else {
            let b = f.widget || (f.block ? xn.block : xn.inline), k = Mm(f), C = this.cache.findWidget(b, a - l, k) || _i.of(b, this.view, a - l, k);
            f.block ? (f.startSide > 0 && r.addLineStartIfNotCovered(i), r.addBlockWidget(C)) : (r.ensureLine(i), r.addInlineWidget(C, u, g));
          }
          i = null;
        } else
          i = Tm(i, f);
        a > l && this.text.skip(a - l);
      },
      span: (l, a, f, u) => {
        for (let g = l; g < a; ) {
          let y = this.text.next(Math.min(512, a - g));
          y == null ? (r.addLineStartIfNotCovered(i), r.addBreak(), g++) : (r.ensureLine(i), r.addText(y, f, u), g += y.length), i = null;
        }
      }
    });
    r.addLineStartIfNotCovered(i), this.openWidget = o > s, this.openMarks = o;
  }
  forward(e, t, i = 1) {
    t - e <= 10 ? this.old.advance(t - e, i, this.reuseWalker) : (this.old.advance(5, -1, this.reuseWalker), this.old.advance(t - e - 10, -1), this.old.advance(5, i, this.reuseWalker));
  }
  getCompositionContext(e) {
    let t = [], i = null;
    for (let r = e.parentNode; ; r = r.parentNode) {
      let s = Ne.get(r);
      if (r == this.view.contentDOM)
        break;
      s instanceof ct ? t.push(s) : s?.isLine() ? i = s : r.nodeName == "DIV" && !i && r != this.view.contentDOM ? i = new bn(r, gu) : t.push(ct.of(new cr({ tagName: r.nodeName.toLowerCase(), attributes: em(r) }), r));
    }
    return { line: i, marks: t };
  }
}
function Th(n, e) {
  let t = (i) => {
    for (let r of i.children)
      if ((e ? r.isText() : r.length) || t(r))
        return !0;
    return !1;
  };
  return t(n);
}
function Mm(n) {
  let e = n.isReplace ? (n.startSide < 0 ? 64 : 0) | (n.endSide > 0 ? 128 : 0) : n.startSide > 0 ? 32 : 16;
  return n.block && (e |= 256), e;
}
const gu = { class: "cm-line" };
function Tm(n, e) {
  let t = e.spec.attributes, i = e.spec.class;
  return !t && !i || (n || (n = { class: "cm-line" }), t && ql(t, n), i && (n.class += " " + i)), n;
}
function Pm(n) {
  let e = [];
  for (let t = n.parents.length; t > 1; t--) {
    let i = t == n.parents.length ? n.tile : n.parents[t].tile;
    i instanceof ct && e.push(i.mark);
  }
  return e;
}
function fo(n) {
  let e = Ne.get(n);
  return e && e.setDOM(n.cloneNode()), n;
}
class xn extends ci {
  constructor(e) {
    super(), this.tag = e;
  }
  eq(e) {
    return e.tag == this.tag;
  }
  toDOM() {
    return document.createElement(this.tag);
  }
  updateDOM(e) {
    return e.nodeName.toLowerCase() == this.tag;
  }
  get isHidden() {
    return !0;
  }
}
xn.inline = /* @__PURE__ */ new xn("span");
xn.block = /* @__PURE__ */ new xn("div");
const uo = /* @__PURE__ */ new class extends ci {
  toDOM() {
    return document.createElement("br");
  }
  get isHidden() {
    return !0;
  }
  get editable() {
    return !0;
  }
}();
class Ph {
  constructor(e) {
    this.view = e, this.decorations = [], this.blockWrappers = [], this.dynamicDecorationMap = [!1], this.domChanged = null, this.hasComposition = null, this.editContextFormatting = G.none, this.lastCompositionAfterCursor = !1, this.minWidth = 0, this.minWidthFrom = 0, this.minWidthTo = 0, this.impreciseAnchor = null, this.impreciseHead = null, this.forceSelection = !1, this.lastUpdate = Date.now(), this.updateDeco(), this.tile = new Ns(e, e.contentDOM), this.updateInner([new Ot(0, 0, 0, e.state.doc.length)], null);
  }
  // Update the document view to a given state.
  update(e) {
    var t;
    let i = e.changedRanges;
    this.minWidth > 0 && i.length && (i.every(({ fromA: u, toA: g }) => g < this.minWidthFrom || u > this.minWidthTo) ? (this.minWidthFrom = e.changes.mapPos(this.minWidthFrom, 1), this.minWidthTo = e.changes.mapPos(this.minWidthTo, 1)) : this.minWidth = this.minWidthFrom = this.minWidthTo = 0), this.updateEditContextFormatting(e);
    let r = -1;
    this.view.inputState.composing >= 0 && !this.view.observer.editContext && (!((t = this.domChanged) === null || t === void 0) && t.newSel ? r = this.domChanged.newSel.head : !Wm(e.changes, this.hasComposition) && !e.selectionSet && (r = e.state.selection.main.head));
    let s = r > -1 ? Lm(this.view, e.changes, r) : null;
    if (this.domChanged = null, this.hasComposition) {
      let { from: u, to: g } = this.hasComposition;
      i = new Ot(u, g, e.changes.mapPos(u, -1), e.changes.mapPos(g, 1)).addToSet(i.slice());
    }
    this.hasComposition = s ? { from: s.range.fromB, to: s.range.toB } : null, (j.ie || j.chrome) && !s && e && e.state.doc.lines != e.startState.doc.lines && (this.forceSelection = !0);
    let o = this.decorations, l = this.blockWrappers;
    this.updateDeco();
    let a = Em(o, this.decorations, e.changes);
    a.length && (i = Ot.extendWithRanges(i, a));
    let f = Nm(l, this.blockWrappers, e.changes);
    return f.length && (i = Ot.extendWithRanges(i, f)), s && !i.some((u) => u.fromA <= s.range.fromA && u.toA >= s.range.toA) && (i = s.range.addToSet(i.slice())), this.tile.flags & 2 && i.length == 0 ? !1 : (this.updateInner(i, s), e.transactions.length && (this.lastUpdate = Date.now()), !0);
  }
  // Used by update and the constructor do perform the actual DOM
  // update
  updateInner(e, t) {
    this.view.viewState.mustMeasureContent = !0;
    let { observer: i } = this.view;
    i.ignore(() => {
      if (t || e.length) {
        let o = this.tile, l = new Am(this.view, o, this.blockWrappers, this.decorations, this.dynamicDecorationMap);
        this.tile = l.run(e, t), hl(o, l.cache.reused);
      }
      this.tile.dom.style.height = this.view.viewState.contentHeight / this.view.scaleY + "px", this.tile.dom.style.flexBasis = this.minWidth ? this.minWidth + "px" : "";
      let s = j.chrome || j.ios ? { node: i.selectionRange.focusNode, written: !1 } : void 0;
      this.tile.sync(s), s && (s.written || i.selectionRange.focusNode != s.node || !this.tile.dom.contains(s.node)) && (this.forceSelection = !0), this.tile.dom.style.height = "";
    });
    let r = [];
    if (this.view.viewport.from || this.view.viewport.to < this.view.state.doc.length)
      for (let s of this.tile.children)
        s.isWidget() && s.widget instanceof po && r.push(s.dom);
    i.updateGaps(r);
  }
  updateEditContextFormatting(e) {
    this.editContextFormatting = this.editContextFormatting.map(e.changes);
    for (let t of e.transactions)
      for (let i of t.effects)
        i.is(cu) && (this.editContextFormatting = i.value);
  }
  // Sync the DOM selection to this.state.selection
  updateSelection(e = !1, t = !1) {
    (e || !this.view.observer.selectionRange.focusNode) && this.view.observer.readSelectionRange();
    let { dom: i } = this.tile, r = this.view.root.activeElement, s = r == i, o = !s && !(this.view.state.facet(ri) || i.tabIndex > -1) && Kn(i, this.view.observer.selectionRange) && !(r && i.contains(r));
    if (!(s || t || o))
      return;
    let l = this.forceSelection;
    this.forceSelection = !1;
    let a = this.view.state.selection.main, f, u;
    if (a.empty ? u = f = this.inlineDOMNearPos(a.anchor, a.assoc || 1) : (u = this.inlineDOMNearPos(a.head, a.head == a.from ? 1 : -1), f = this.inlineDOMNearPos(a.anchor, a.anchor == a.from ? 1 : -1)), j.gecko && a.empty && !this.hasComposition && Rm(f)) {
      let y = document.createTextNode("");
      this.view.observer.ignore(() => f.node.insertBefore(y, f.node.childNodes[f.offset] || null)), f = u = new Nt(y, 0), l = !0;
    }
    let g = this.view.observer.selectionRange;
    (l || !g.focusNode || (!_n(f.node, f.offset, g.anchorNode, g.anchorOffset) || !_n(u.node, u.offset, g.focusNode, g.focusOffset)) && !this.suppressWidgetCursorChange(g, a)) && (this.view.observer.ignore(() => {
      j.android && j.chrome && i.contains(g.focusNode) && Fm(g.focusNode, i) && (i.blur(), i.focus({ preventScroll: !0 }));
      let y = yn(this.view.root);
      if (y) if (a.empty) {
        if (j.gecko) {
          let b = Dm(f.node, f.offset);
          if (b && b != 3) {
            let k = (b == 1 ? Uf : Xf)(f.node, f.offset);
            k && (f = new Nt(k.node, k.offset));
          }
        }
        y.collapse(f.node, f.offset), a.bidiLevel != null && y.caretBidiLevel !== void 0 && (y.caretBidiLevel = a.bidiLevel);
      } else if (y.extend) {
        y.collapse(f.node, f.offset);
        try {
          y.extend(u.node, u.offset);
        } catch {
        }
      } else {
        let b = document.createRange();
        a.anchor > a.head && ([f, u] = [u, f]), b.setEnd(u.node, u.offset), b.setStart(f.node, f.offset), y.removeAllRanges(), y.addRange(b);
      }
      o && this.view.root.activeElement == i && (i.blur(), r && r.focus());
    }), this.view.observer.setSelectionRange(f, u)), this.impreciseAnchor = f.precise ? null : new Nt(g.anchorNode, g.anchorOffset), this.impreciseHead = u.precise ? null : new Nt(g.focusNode, g.focusOffset);
  }
  // If a zero-length widget is inserted next to the cursor during
  // composition, avoid moving it across it and disrupting the
  // composition.
  suppressWidgetCursorChange(e, t) {
    return this.hasComposition && t.empty && _n(e.focusNode, e.focusOffset, e.anchorNode, e.anchorOffset) && this.posFromDOM(e.focusNode, e.focusOffset) == t.head;
  }
  enforceCursorAssoc() {
    if (this.hasComposition)
      return;
    let { view: e } = this, t = e.state.selection.main, i = yn(e.root), { anchorNode: r, anchorOffset: s } = e.observer.selectionRange;
    if (!i || !t.empty || !t.assoc || !i.modify)
      return;
    let o = this.lineAt(t.head, t.assoc);
    if (!o)
      return;
    let l = o.posAtStart;
    if (t.head == l || t.head == l + o.length)
      return;
    let a = this.coordsAt(t.head, -1), f = this.coordsAt(t.head, 1);
    if (!a || !f || a.bottom > f.top)
      return;
    let u = this.domAtPos(t.head + t.assoc, t.assoc);
    i.collapse(u.node, u.offset), i.modify("move", t.assoc < 0 ? "forward" : "backward", "lineboundary"), e.observer.readSelectionRange();
    let g = e.observer.selectionRange;
    e.docView.posFromDOM(g.anchorNode, g.anchorOffset) != t.from && i.collapse(r, s);
  }
  posFromDOM(e, t) {
    let i = this.tile.nearest(e);
    if (!i)
      return this.tile.dom.compareDocumentPosition(e) & 2 ? 0 : this.view.state.doc.length;
    let r = i.posAtStart;
    if (i.isComposite()) {
      let s;
      if (e == i.dom)
        s = i.dom.childNodes[t];
      else {
        let o = li(e) == 0 ? 0 : t == 0 ? -1 : 1;
        for (; ; ) {
          let l = e.parentNode;
          if (l == i.dom)
            break;
          o == 0 && l.firstChild != l.lastChild && (e == l.firstChild ? o = -1 : o = 1), e = l;
        }
        o < 0 ? s = e : s = e.nextSibling;
      }
      if (s == i.dom.firstChild)
        return r;
      for (; s && !Ne.get(s); )
        s = s.nextSibling;
      if (!s)
        return r + i.length;
      for (let o = 0, l = r; ; o++) {
        let a = i.children[o];
        if (a.dom == s)
          return l;
        l += a.length + a.breakAfter;
      }
    } else return i.isText() ? e == i.dom ? r + t : r + (t ? i.length : 0) : r;
  }
  domAtPos(e, t) {
    let { tile: i, offset: r } = this.tile.resolveBlock(e, t);
    return i.isWidget() ? i.domPosFor(e, t) : i.domIn(r, t);
  }
  inlineDOMNearPos(e, t) {
    let i, r = -1, s = !1, o, l = -1, a = !1;
    return this.tile.blockTiles((f, u) => {
      if (f.isWidget()) {
        if (f.flags & 32 && u >= e)
          return !0;
        f.flags & 16 && (s = !0);
      } else {
        let g = u + f.length;
        if (u <= e && (i = f, r = e - u, s = g < e), g >= e && !o && (o = f, l = e - u, a = u > e), u > e && o)
          return !0;
      }
    }), !i && !o ? this.domAtPos(e, t) : (s && o ? i = null : a && i && (o = null), i && t < 0 || !o ? i.domIn(r, t) : o.domIn(l, t));
  }
  coordsAt(e, t) {
    let { tile: i, offset: r } = this.tile.resolveBlock(e, t);
    return i.isWidget() ? i.widget instanceof po ? null : i.coordsInWidget(r, t, !0) : i.coordsIn(r, t);
  }
  lineAt(e, t) {
    let { tile: i } = this.tile.resolveBlock(e, t);
    return i.isLine() ? i : null;
  }
  coordsForChar(e) {
    let { tile: t, offset: i } = this.tile.resolveBlock(e, 1);
    if (!t.isLine())
      return null;
    function r(s, o) {
      if (s.isComposite())
        for (let l of s.children) {
          if (l.length >= o) {
            let a = r(l, o);
            if (a)
              return a;
          }
          if (o -= l.length, o < 0)
            break;
        }
      else if (s.isText() && o < s.length) {
        let l = We(s.text, o);
        if (l == o)
          return null;
        let a = Jn(s.dom, o, l).getClientRects();
        for (let f = 0; f < a.length; f++) {
          let u = a[f];
          if (f == a.length - 1 || u.top < u.bottom && u.left < u.right)
            return u;
        }
      }
      return null;
    }
    return r(t, i);
  }
  measureVisibleLineHeights(e) {
    let t = [], { from: i, to: r } = e, s = this.view.contentDOM.clientWidth, o = s > Math.max(this.view.scrollDOM.clientWidth, this.minWidth) + 1, l = -1, a = this.view.textDirection == xe.LTR, f = 0, u = (g, y, b) => {
      for (let k = 0; k < g.children.length && !(y > r); k++) {
        let C = g.children[k], M = y + C.length, R = C.dom.getBoundingClientRect(), { height: I } = R;
        if (b && !k && (f += R.top - b.top), C instanceof bi)
          M > i && u(C, y, R);
        else if (y >= i && (f > 0 && t.push(-f), t.push(I + f), f = 0, o)) {
          let N = C.dom.lastChild, z = N ? jr(N) : [];
          if (z.length) {
            let F = z[z.length - 1], H = a ? F.right - R.left : R.right - F.left;
            H > l && (l = H, this.minWidth = s, this.minWidthFrom = y, this.minWidthTo = M);
          }
        }
        b && k == g.children.length - 1 && (f += b.bottom - R.bottom), y = M + C.breakAfter;
      }
    };
    return u(this.tile, 0, null), t;
  }
  textDirectionAt(e) {
    let { tile: t } = this.tile.resolveBlock(e, 1);
    return getComputedStyle(t.dom).direction == "rtl" ? xe.RTL : xe.LTR;
  }
  measureTextSize() {
    let e = this.tile.blockTiles((o) => {
      if (o.isLine() && o.children.length && o.length <= 20) {
        let l = 0, a;
        for (let f of o.children) {
          if (!f.isText() || /[^ -~]/.test(f.text))
            return;
          let u = jr(f.dom);
          if (u.length != 1)
            return;
          l += u[0].width, a = u[0].height;
        }
        if (l)
          return {
            lineHeight: o.dom.getBoundingClientRect().height,
            charWidth: l / o.length,
            textHeight: a
          };
      }
    });
    if (e)
      return e;
    let t = document.createElement("div"), i, r, s;
    return t.className = "cm-line", t.style.width = "99999px", t.style.position = "absolute", t.textContent = "abc def ghi jkl mno pqr stu", this.view.observer.ignore(() => {
      this.tile.dom.appendChild(t);
      let o = jr(t.firstChild)[0];
      i = t.getBoundingClientRect().height, r = o && o.width ? o.width / 27 : 7, s = o && o.height ? o.height : i, t.remove();
    }), { lineHeight: i, charWidth: r, textHeight: s };
  }
  computeBlockGapDeco() {
    let e = [], t = this.view.viewState;
    for (let i = 0, r = 0; ; r++) {
      let s = r == t.viewports.length ? null : t.viewports[r], o = s ? s.from - 1 : this.view.state.doc.length;
      if (o > i) {
        let l = (t.lineBlockAt(o).bottom - t.lineBlockAt(i).top) / this.view.scaleY;
        e.push(G.replace({
          widget: new po(l),
          block: !0,
          inclusive: !0,
          isBlockGap: !0
        }).range(i, o));
      }
      if (!s)
        break;
      i = s.to + 1;
    }
    return G.set(e);
  }
  updateDeco() {
    let e = 1, t = this.view.state.facet(Es).map((s) => (this.dynamicDecorationMap[e++] = typeof s == "function") ? s(this.view) : s), i = !1, r = this.view.state.facet(Yl).map((s, o) => {
      let l = typeof s == "function";
      return l && (i = !0), l ? s(this.view) : s;
    });
    for (r.length && (this.dynamicDecorationMap[e++] = i, t.push(ce.join(r))), this.decorations = [
      this.editContextFormatting,
      ...t,
      this.computeBlockGapDeco(),
      this.view.viewState.lineGapDeco
    ]; e < this.decorations.length; )
      this.dynamicDecorationMap[e++] = !1;
    this.blockWrappers = this.view.state.facet(uu).map((s) => typeof s == "function" ? s(this.view) : s);
  }
  scrollIntoView(e) {
    if (e.isSnapshot) {
      let f = this.view.viewState.lineBlockAt(e.range.head);
      this.view.scrollDOM.scrollTop = f.top - e.yMargin, this.view.scrollDOM.scrollLeft = e.xMargin;
      return;
    }
    for (let f of this.view.state.facet(hu))
      try {
        if (f(this.view, e.range, e))
          return !0;
      } catch (u) {
        ft(this.view.state, u, "scroll handler");
      }
    let { range: t } = e, i = this.coordsAt(t.head, t.empty ? t.assoc : t.head > t.anchor ? -1 : 1), r;
    if (!i)
      return;
    !t.empty && (r = this.coordsAt(t.anchor, t.anchor > t.head ? -1 : 1)) && (i = {
      left: Math.min(i.left, r.left),
      top: Math.min(i.top, r.top),
      right: Math.max(i.right, r.right),
      bottom: Math.max(i.bottom, r.bottom)
    });
    let s = Gl(this.view), o = {
      left: i.left - s.left,
      top: i.top - s.top,
      right: i.right + s.right,
      bottom: i.bottom + s.bottom
    }, { offsetWidth: l, offsetHeight: a } = this.view.scrollDOM;
    nm(this.view.scrollDOM, o, t.head < t.anchor ? -1 : 1, e.x, e.y, Math.max(Math.min(e.xMargin, l), -l), Math.max(Math.min(e.yMargin, a), -a), this.view.textDirection == xe.LTR);
  }
  lineHasWidget(e) {
    let t = (i) => i.isWidget() || i.children.some(t);
    return t(this.tile.resolveBlock(e, 1).tile);
  }
  destroy() {
    hl(this.tile);
  }
}
function hl(n, e) {
  let t = e?.get(n);
  if (t != 1) {
    t == null && n.destroy();
    for (let i of n.children)
      hl(i, e);
  }
}
function Rm(n) {
  return n.node.nodeType == 1 && n.node.firstChild && (n.offset == 0 || n.node.childNodes[n.offset - 1].contentEditable == "false") && (n.offset == n.node.childNodes.length || n.node.childNodes[n.offset].contentEditable == "false");
}
function mu(n, e) {
  let t = n.observer.selectionRange;
  if (!t.focusNode)
    return null;
  let i = Uf(t.focusNode, t.focusOffset), r = Xf(t.focusNode, t.focusOffset), s = i || r;
  if (r && i && r.node != i.node) {
    let l = Ne.get(r.node);
    if (!l || l.isText() && l.text != r.node.nodeValue)
      s = r;
    else if (n.docView.lastCompositionAfterCursor) {
      let a = Ne.get(i.node);
      !a || a.isText() && a.text != i.node.nodeValue || (s = r);
    }
  }
  if (n.docView.lastCompositionAfterCursor = s != i, !s)
    return null;
  let o = e - s.offset;
  return { from: o, to: o + s.node.nodeValue.length, node: s.node };
}
function Lm(n, e, t) {
  let i = mu(n, t);
  if (!i)
    return null;
  let { node: r, from: s, to: o } = i, l = r.nodeValue;
  if (/[\n\r]/.test(l) || n.state.doc.sliceString(i.from, i.to) != l)
    return null;
  let a = e.invertedDesc;
  return { range: new Ot(a.mapPos(s), a.mapPos(o), s, o), text: r };
}
function Dm(n, e) {
  return n.nodeType != 1 ? 0 : (e && n.childNodes[e - 1].contentEditable == "false" ? 1 : 0) | (e < n.childNodes.length && n.childNodes[e].contentEditable == "false" ? 2 : 0);
}
let Bm = class {
  constructor() {
    this.changes = [];
  }
  compareRange(e, t) {
    cn(e, t, this.changes);
  }
  comparePoint(e, t) {
    cn(e, t, this.changes);
  }
  boundChange(e) {
    cn(e, e, this.changes);
  }
};
function Em(n, e, t) {
  let i = new Bm();
  return ce.compare(n, e, t, i), i.changes;
}
class Im {
  constructor() {
    this.changes = [];
  }
  compareRange(e, t) {
    cn(e, t, this.changes);
  }
  comparePoint() {
  }
  boundChange(e) {
    cn(e, e, this.changes);
  }
}
function Nm(n, e, t) {
  let i = new Im();
  return ce.compare(n, e, t, i), i.changes;
}
function Fm(n, e) {
  for (let t = n; t && t != e; t = t.assignedSlot || t.parentNode)
    if (t.nodeType == 1 && t.contentEditable == "false")
      return !0;
  return !1;
}
function Wm(n, e) {
  let t = !1;
  return e && n.iterChangedRanges((i, r) => {
    i < e.to && r > e.from && (t = !0);
  }), t;
}
class po extends ci {
  constructor(e) {
    super(), this.height = e;
  }
  toDOM() {
    let e = document.createElement("div");
    return e.className = "cm-gap", this.updateDOM(e), e;
  }
  eq(e) {
    return e.height == this.height;
  }
  updateDOM(e) {
    return e.style.height = this.height + "px", !0;
  }
  get editable() {
    return !0;
  }
  get estimatedHeight() {
    return this.height;
  }
  ignoreEvent() {
    return !1;
  }
}
function Qm(n, e, t = 1) {
  let i = n.charCategorizer(e), r = n.doc.lineAt(e), s = e - r.from;
  if (r.length == 0)
    return E.cursor(e);
  s == 0 ? t = 1 : s == r.length && (t = -1);
  let o = s, l = s;
  t < 0 ? o = We(r.text, s, !1) : l = We(r.text, s);
  let a = i(r.text.slice(o, l));
  for (; o > 0; ) {
    let f = We(r.text, o, !1);
    if (i(r.text.slice(f, o)) != a)
      break;
    o = f;
  }
  for (; l < r.length; ) {
    let f = We(r.text, l);
    if (i(r.text.slice(l, f)) != a)
      break;
    l = f;
  }
  return E.range(o + r.from, l + r.from);
}
function Vm(n, e, t, i, r) {
  let s = Math.round((i - e.left) * n.defaultCharacterWidth);
  if (n.lineWrapping && t.height > n.defaultLineHeight * 1.5) {
    let l = n.viewState.heightOracle.textHeight, a = Math.floor((r - t.top - (n.defaultLineHeight - l) * 0.5) / l);
    s += a * n.viewState.heightOracle.lineLength;
  }
  let o = n.state.sliceDoc(t.from, t.to);
  return t.from + Go(o, s, n.state.tabSize);
}
function cl(n, e, t) {
  let i = n.lineBlockAt(e);
  if (Array.isArray(i.type)) {
    let r;
    for (let s of i.type) {
      if (s.from > e)
        break;
      if (!(s.to < e)) {
        if (s.from < e && s.to > e)
          return s;
        (!r || s.type == Ye.Text && (r.type != s.type || (t < 0 ? s.from < e : s.to > e))) && (r = s);
      }
    }
    return r || i;
  }
  return i;
}
function Hm(n, e, t, i) {
  let r = cl(n, e.head, e.assoc || -1), s = !i || r.type != Ye.Text || !(n.lineWrapping || r.widgetLineBreaks) ? null : n.coordsAtPos(e.assoc < 0 && e.head > r.from ? e.head - 1 : e.head);
  if (s) {
    let o = n.dom.getBoundingClientRect(), l = n.textDirectionAt(r.from), a = n.posAtCoords({
      x: t == (l == xe.LTR) ? o.right - 1 : o.left + 1,
      y: (s.top + s.bottom) / 2
    });
    if (a != null)
      return E.cursor(a, t ? -1 : 1);
  }
  return E.cursor(t ? r.to : r.from, t ? -1 : 1);
}
function Rh(n, e, t, i) {
  let r = n.state.doc.lineAt(e.head), s = n.bidiSpans(r), o = n.textDirectionAt(r.from);
  for (let l = e, a = null; ; ) {
    let f = gm(r, s, o, l, t), u = eu;
    if (!f) {
      if (r.number == (t ? n.state.doc.lines : 1))
        return l;
      u = `
`, r = n.state.doc.line(r.number + (t ? 1 : -1)), s = n.bidiSpans(r), f = n.visualLineSide(r, !t);
    }
    if (a) {
      if (!a(u))
        return l;
    } else {
      if (!i)
        return f;
      a = i(u);
    }
    l = f;
  }
}
function zm(n, e, t) {
  let i = n.state.charCategorizer(e), r = i(t);
  return (s) => {
    let o = i(s);
    return r == Me.Space && (r = o), r == o;
  };
}
function $m(n, e, t, i) {
  let r = e.head, s = t ? 1 : -1;
  if (r == (t ? n.state.doc.length : 0))
    return E.cursor(r, e.assoc);
  let o = e.goalColumn, l, a = n.contentDOM.getBoundingClientRect(), f = n.coordsAtPos(r, e.assoc || -1), u = n.documentTop;
  if (f)
    o == null && (o = f.left - a.left), l = s < 0 ? f.top : f.bottom;
  else {
    let k = n.viewState.lineBlockAt(r);
    o == null && (o = Math.min(a.right - a.left, n.defaultCharacterWidth * (r - k.from))), l = (s < 0 ? k.top : k.bottom) + u;
  }
  let g = a.left + o, y = i ?? n.viewState.heightOracle.textHeight >> 1, b = fl(n, { x: g, y: l + y * s }, !1, s);
  return E.cursor(b.pos, b.assoc, void 0, o);
}
function jn(n, e, t) {
  for (; ; ) {
    let i = 0;
    for (let r of n)
      r.between(e - 1, e + 1, (s, o, l) => {
        if (e > s && e < o) {
          let a = i || t || (e - s < o - e ? -1 : 1);
          e = a < 0 ? s : o, i = a;
        }
      });
    if (!i)
      return e;
  }
}
function vu(n, e) {
  let t = null;
  for (let i = 0; i < e.ranges.length; i++) {
    let r = e.ranges[i], s = null;
    if (r.empty) {
      let o = jn(n, r.from, 0);
      o != r.from && (s = E.cursor(o, -1));
    } else {
      let o = jn(n, r.from, -1), l = jn(n, r.to, 1);
      (o != r.from || l != r.to) && (s = E.range(r.from == r.anchor ? o : l, r.from == r.head ? o : l));
    }
    s && (t || (t = e.ranges.slice()), t[i] = s);
  }
  return t ? E.create(t, e.mainIndex) : e;
}
function go(n, e, t) {
  let i = jn(n.state.facet(ur).map((r) => r(n)), t.from, e.head > t.from ? -1 : 1);
  return i == t.from ? t : E.cursor(i, i < t.from ? 1 : -1);
}
class Gt {
  constructor(e, t) {
    this.pos = e, this.assoc = t;
  }
}
function fl(n, e, t, i) {
  let r = n.contentDOM.getBoundingClientRect(), s = r.top + n.viewState.paddingTop, { x: o, y: l } = e, a = l - s, f;
  for (; ; ) {
    if (a < 0)
      return new Gt(0, 1);
    if (a > n.viewState.docHeight)
      return new Gt(n.state.doc.length, -1);
    if (f = n.elementAtHeight(a), i == null)
      break;
    if (f.type == Ye.Text) {
      let y = n.docView.coordsAt(i < 0 ? f.from : f.to, i);
      if (y && (i < 0 ? y.top <= a + s : y.bottom >= a + s))
        break;
    }
    let g = n.viewState.heightOracle.textHeight / 2;
    a = i > 0 ? f.bottom + g : f.top - g;
  }
  if (n.viewport.from >= f.to || n.viewport.to <= f.from) {
    if (t)
      return null;
    if (f.type == Ye.Text) {
      let g = Vm(n, r, f, o, l);
      return new Gt(g, g == f.from ? 1 : -1);
    }
  }
  if (f.type != Ye.Text)
    return a < (f.top + f.bottom) / 2 ? new Gt(f.from, 1) : new Gt(f.to, -1);
  let u = n.docView.lineAt(f.from, 2);
  return (!u || u.length != f.length) && (u = n.docView.lineAt(f.from, -2)), yu(n, u, f.from, o, l);
}
function yu(n, e, t, i, r) {
  let s = -1, o = null, l = 1e9, a = 1e9, f = r, u = r, g = (y, b) => {
    for (let k = 0; k < y.length; k++) {
      let C = y[k];
      if (C.top == C.bottom)
        continue;
      let M = C.left > i ? C.left - i : C.right < i ? i - C.right : 0, R = C.top > r ? C.top - r : C.bottom < r ? r - C.bottom : 0;
      C.top <= u && C.bottom >= f && (f = Math.min(C.top, f), u = Math.max(C.bottom, u), R = 0), (s < 0 || (R - a || M - l) < 0) && (s >= 0 && a && l < M && o.top <= u - 2 && o.bottom >= f + 2 ? a = 0 : (s = b, l = M, a = R, o = C));
    }
  };
  if (e.isText()) {
    for (let b = 0; b < e.length; ) {
      let k = We(e.text, b);
      if (g(Jn(e.dom, b, k).getClientRects(), b), !l && !a)
        break;
      b = k;
    }
    return i > (o.left + o.right) / 2 == (Lh(n, s + t) == xe.LTR) ? new Gt(t + We(e.text, s), -1) : new Gt(t + s, 1);
  } else {
    if (!e.length)
      return new Gt(t, 1);
    for (let C = 0; C < e.children.length; C++) {
      let M = e.children[C];
      if (M.flags & 48)
        continue;
      let R = (M.dom.nodeType == 1 ? M.dom : Jn(M.dom, 0, M.length)).getClientRects();
      if (g(R, C), !l && !a)
        break;
    }
    let y = e.children[s], b = e.posBefore(y, t);
    return y.isComposite() || y.isText() ? yu(n, y, b, Math.max(o.left, Math.min(o.right, i)), r) : i > (o.left + o.right) / 2 == (Lh(n, s + t) == xe.LTR) ? new Gt(b + y.length, -1) : new Gt(b, 1);
  }
}
function Lh(n, e) {
  let t = n.state.doc.lineAt(e);
  return n.bidiSpans(t)[si.find(n.bidiSpans(t), e - t.from, -1, 1)].dir;
}
const Qn = "￿";
class qm {
  constructor(e, t) {
    this.points = e, this.view = t, this.text = "", this.lineSeparator = t.state.facet(pe.lineSeparator);
  }
  append(e) {
    this.text += e;
  }
  lineBreak() {
    this.text += Qn;
  }
  readRange(e, t) {
    if (!e)
      return this;
    let i = e.parentNode;
    for (let r = e; ; ) {
      this.findPointBefore(i, r);
      let s = this.text.length;
      this.readNode(r);
      let o = Ne.get(r), l = r.nextSibling;
      if (l == t) {
        o?.breakAfter && !l && i != this.view.contentDOM && this.lineBreak();
        break;
      }
      let a = Ne.get(l);
      (o && a ? o.breakAfter : (o ? o.breakAfter : rs(r)) || rs(l) && (r.nodeName != "BR" || o?.isWidget()) && this.text.length > s) && !_m(l, t) && this.lineBreak(), r = l;
    }
    return this.findPointBefore(i, t), this;
  }
  readTextNode(e) {
    let t = e.nodeValue;
    for (let i of this.points)
      i.node == e && (i.pos = this.text.length + Math.min(i.offset, t.length));
    for (let i = 0, r = this.lineSeparator ? null : /\r\n?|\n/g; ; ) {
      let s = -1, o = 1, l;
      if (this.lineSeparator ? (s = t.indexOf(this.lineSeparator, i), o = this.lineSeparator.length) : (l = r.exec(t)) && (s = l.index, o = l[0].length), this.append(t.slice(i, s < 0 ? t.length : s)), s < 0)
        break;
      if (this.lineBreak(), o > 1)
        for (let a of this.points)
          a.node == e && a.pos > this.text.length && (a.pos -= o - 1);
      i = s + o;
    }
  }
  readNode(e) {
    let t = Ne.get(e), i = t && t.overrideDOMText;
    if (i != null) {
      this.findPointInside(e, i.length);
      for (let r = i.iter(); !r.next().done; )
        r.lineBreak ? this.lineBreak() : this.append(r.value);
    } else e.nodeType == 3 ? this.readTextNode(e) : e.nodeName == "BR" ? e.nextSibling && this.lineBreak() : e.nodeType == 1 && this.readRange(e.firstChild, null);
  }
  findPointBefore(e, t) {
    for (let i of this.points)
      i.node == e && e.childNodes[i.offset] == t && (i.pos = this.text.length);
  }
  findPointInside(e, t) {
    for (let i of this.points)
      (e.nodeType == 3 ? i.node == e : e.contains(i.node)) && (i.pos = this.text.length + (Km(e, i.node, i.offset) ? t : 0));
  }
}
function Km(n, e, t) {
  for (; ; ) {
    if (!e || t < li(e))
      return !1;
    if (e == n)
      return !0;
    t = Ci(e) + 1, e = e.parentNode;
  }
}
function _m(n, e) {
  let t;
  for (; !(n == e || !n); n = n.nextSibling) {
    let i = Ne.get(n);
    if (!i?.isWidget())
      return !1;
    i && (t || (t = [])).push(i);
  }
  if (t)
    for (let i of t) {
      let r = i.overrideDOMText;
      if (r?.length)
        return !1;
    }
  return !0;
}
class Dh {
  constructor(e, t) {
    this.node = e, this.offset = t, this.pos = -1;
  }
}
class jm {
  constructor(e, t, i, r) {
    this.typeOver = r, this.bounds = null, this.text = "", this.domChanged = t > -1;
    let { impreciseHead: s, impreciseAnchor: o } = e.docView;
    if (e.state.readOnly && t > -1)
      this.newSel = null;
    else if (t > -1 && (this.bounds = bu(e.docView.tile, t, i, 0))) {
      let l = s || o ? [] : Xm(e), a = new qm(l, e);
      a.readRange(this.bounds.startDOM, this.bounds.endDOM), this.text = a.text, this.newSel = Ym(l, this.bounds.from);
    } else {
      let l = e.observer.selectionRange, a = s && s.node == l.focusNode && s.offset == l.focusOffset || !rl(e.contentDOM, l.focusNode) ? e.state.selection.main.head : e.docView.posFromDOM(l.focusNode, l.focusOffset), f = o && o.node == l.anchorNode && o.offset == l.anchorOffset || !rl(e.contentDOM, l.anchorNode) ? e.state.selection.main.anchor : e.docView.posFromDOM(l.anchorNode, l.anchorOffset), u = e.viewport;
      if ((j.ios || j.chrome) && e.state.selection.main.empty && a != f && (u.from > 0 || u.to < e.state.doc.length)) {
        let g = Math.min(a, f), y = Math.max(a, f), b = u.from - g, k = u.to - y;
        (b == 0 || b == 1 || g == 0) && (k == 0 || k == -1 || y == e.state.doc.length) && (a = 0, f = e.state.doc.length);
      }
      e.inputState.composing > -1 && e.state.selection.ranges.length > 1 ? this.newSel = e.state.selection.replaceRange(E.range(f, a)) : this.newSel = E.single(f, a);
    }
  }
}
function bu(n, e, t, i) {
  if (n.isComposite()) {
    let r = -1, s = -1, o = -1, l = -1;
    for (let a = 0, f = i, u = i; a < n.children.length; a++) {
      let g = n.children[a], y = f + g.length;
      if (f < e && y > t)
        return bu(g, e, t, f);
      if (y >= e && r == -1 && (r = a, s = f), f > t && g.dom.parentNode == n.dom) {
        o = a, l = u;
        break;
      }
      u = y, f = y + g.breakAfter;
    }
    return {
      from: s,
      to: l < 0 ? i + n.length : l,
      startDOM: (r ? n.children[r - 1].dom.nextSibling : null) || n.dom.firstChild,
      endDOM: o < n.children.length && o >= 0 ? n.children[o].dom : null
    };
  } else return n.isText() ? { from: i, to: i + n.length, startDOM: n.dom, endDOM: n.dom.nextSibling } : null;
}
function xu(n, e) {
  let t, { newSel: i } = e, r = n.state.selection.main, s = n.inputState.lastKeyTime > Date.now() - 100 ? n.inputState.lastKeyCode : -1;
  if (e.bounds) {
    let { from: o, to: l } = e.bounds, a = r.from, f = null;
    (s === 8 || j.android && e.text.length < l - o) && (a = r.to, f = "end");
    let u = ku(n.state.doc.sliceString(o, l, Qn), e.text, a - o, f);
    u && (j.chrome && s == 13 && u.toB == u.from + 2 && e.text.slice(u.from, u.toB) == Qn + Qn && u.toB--, t = {
      from: o + u.from,
      to: o + u.toA,
      insert: ge.of(e.text.slice(u.from, u.toB).split(Qn))
    });
  } else i && (!n.hasFocus && n.state.facet(ri) || hs(i, r)) && (i = null);
  if (!t && !i)
    return !1;
  if (!t && e.typeOver && !r.empty && i && i.main.empty ? t = { from: r.from, to: r.to, insert: n.state.doc.slice(r.from, r.to) } : (j.mac || j.android) && t && t.from == t.to && t.from == r.head - 1 && /^\. ?$/.test(t.insert.toString()) && n.contentDOM.getAttribute("autocorrect") == "off" ? (i && t.insert.length == 2 && (i = E.single(i.main.anchor - 1, i.main.head - 1)), t = { from: t.from, to: t.to, insert: ge.of([t.insert.toString().replace(".", " ")]) }) : t && t.from >= r.from && t.to <= r.to && (t.from != r.from || t.to != r.to) && r.to - r.from - (t.to - t.from) <= 4 ? t = {
    from: r.from,
    to: r.to,
    insert: n.state.doc.slice(r.from, t.from).append(t.insert).append(n.state.doc.slice(t.to, r.to))
  } : n.state.doc.lineAt(r.from).to < r.to && n.docView.lineHasWidget(r.to) && n.inputState.insertingTextAt > Date.now() - 50 ? t = {
    from: r.from,
    to: r.to,
    insert: n.state.toText(n.inputState.insertingText)
  } : j.chrome && t && t.from == t.to && t.from == r.head && t.insert.toString() == `
 ` && n.lineWrapping && (i && (i = E.single(i.main.anchor - 1, i.main.head - 1)), t = { from: r.from, to: r.to, insert: ge.of([" "]) }), t)
    return Zl(n, t, i, s);
  if (i && !hs(i, r)) {
    let o = !1, l = "select";
    return n.inputState.lastSelectionTime > Date.now() - 50 && (n.inputState.lastSelectionOrigin == "select" && (o = !0), l = n.inputState.lastSelectionOrigin, l == "select.pointer" && (i = vu(n.state.facet(ur).map((a) => a(n)), i))), n.dispatch({ selection: i, scrollIntoView: o, userEvent: l }), !0;
  } else
    return !1;
}
function Zl(n, e, t, i = -1) {
  if (j.ios && n.inputState.flushIOSKey(e))
    return !0;
  let r = n.state.selection.main;
  if (j.android && (e.to == r.to && // GBoard will sometimes remove a space it just inserted
  // after a completion when you press enter
  (e.from == r.from || e.from == r.from - 1 && n.state.sliceDoc(e.from, r.from) == " ") && e.insert.length == 1 && e.insert.lines == 2 && fn(n.contentDOM, "Enter", 13) || (e.from == r.from - 1 && e.to == r.to && e.insert.length == 0 || i == 8 && e.insert.length < e.to - e.from && e.to > r.head) && fn(n.contentDOM, "Backspace", 8) || e.from == r.from && e.to == r.to + 1 && e.insert.length == 0 && fn(n.contentDOM, "Delete", 46)))
    return !0;
  let s = e.insert.toString();
  n.inputState.composing >= 0 && n.inputState.composing++;
  let o, l = () => o || (o = Um(n, e, t));
  return n.state.facet(su).some((a) => a(n, e.from, e.to, s, l)) || n.dispatch(l()), !0;
}
function Um(n, e, t) {
  let i, r = n.state, s = r.selection.main, o = -1;
  if (e.from == e.to && e.from < s.from || e.from > s.to) {
    let a = e.from < s.from ? -1 : 1, f = a < 0 ? s.from : s.to, u = jn(r.facet(ur).map((g) => g(n)), f, a);
    e.from == u && (o = u);
  }
  if (o > -1)
    i = {
      changes: e,
      selection: E.cursor(e.from + e.insert.length, -1)
    };
  else if (e.from >= s.from && e.to <= s.to && e.to - e.from >= (s.to - s.from) / 3 && (!t || t.main.empty && t.main.from == e.from + e.insert.length) && n.inputState.composing < 0) {
    let a = s.from < e.from ? r.sliceDoc(s.from, e.from) : "", f = s.to > e.to ? r.sliceDoc(e.to, s.to) : "";
    i = r.replaceSelection(n.state.toText(a + e.insert.sliceString(0, void 0, n.state.lineBreak) + f));
  } else {
    let a = r.changes(e), f = t && t.main.to <= a.newLength ? t.main : void 0;
    if (r.selection.ranges.length > 1 && (n.inputState.composing >= 0 || n.inputState.compositionPendingChange) && e.to <= s.to + 10 && e.to >= s.to - 10) {
      let u = n.state.sliceDoc(e.from, e.to), g, y = t && mu(n, t.main.head);
      if (y) {
        let k = e.insert.length - (e.to - e.from);
        g = { from: y.from, to: y.to - k };
      } else
        g = n.state.doc.lineAt(s.head);
      let b = s.to - e.to;
      i = r.changeByRange((k) => {
        if (k.from == s.from && k.to == s.to)
          return { changes: a, range: f || k.map(a) };
        let C = k.to - b, M = C - u.length;
        if (n.state.sliceDoc(M, C) != u || // Unfortunately, there's no way to make multiple
        // changes in the same node work without aborting
        // composition, so cursors in the composition range are
        // ignored.
        C >= g.from && M <= g.to)
          return { range: k };
        let R = r.changes({ from: M, to: C, insert: e.insert }), I = k.to - s.to;
        return {
          changes: R,
          range: f ? E.range(Math.max(0, f.anchor + I), Math.max(0, f.head + I)) : k.map(R)
        };
      });
    } else
      i = {
        changes: a,
        selection: f && r.selection.replaceRange(f)
      };
  }
  let l = "input.type";
  return (n.composing || n.inputState.compositionPendingChange && n.inputState.compositionEndedAt > Date.now() - 50) && (n.inputState.compositionPendingChange = !1, l += ".compose", n.inputState.compositionFirstChange && (l += ".start", n.inputState.compositionFirstChange = !1)), r.update(i, { userEvent: l, scrollIntoView: !0 });
}
function ku(n, e, t, i) {
  let r = Math.min(n.length, e.length), s = 0;
  for (; s < r && n.charCodeAt(s) == e.charCodeAt(s); )
    s++;
  if (s == r && n.length == e.length)
    return null;
  let o = n.length, l = e.length;
  for (; o > 0 && l > 0 && n.charCodeAt(o - 1) == e.charCodeAt(l - 1); )
    o--, l--;
  if (i == "end") {
    let a = Math.max(0, s - Math.min(o, l));
    t -= o + a - s;
  }
  if (o < s && n.length < e.length) {
    let a = t <= s && t >= o ? s - t : 0;
    s -= a, l = s + (l - o), o = s;
  } else if (l < s) {
    let a = t <= s && t >= l ? s - t : 0;
    s -= a, o = s + (o - l), l = s;
  }
  return { from: s, toA: o, toB: l };
}
function Xm(n) {
  let e = [];
  if (n.root.activeElement != n.contentDOM)
    return e;
  let { anchorNode: t, anchorOffset: i, focusNode: r, focusOffset: s } = n.observer.selectionRange;
  return t && (e.push(new Dh(t, i)), (r != t || s != i) && e.push(new Dh(r, s))), e;
}
function Ym(n, e) {
  if (n.length == 0)
    return null;
  let t = n[0].pos, i = n.length == 2 ? n[1].pos : t;
  return t > -1 && i > -1 ? E.single(t + e, i + e) : null;
}
function hs(n, e) {
  return e.head == n.main.head && e.anchor == n.main.anchor;
}
class Gm {
  setSelectionOrigin(e) {
    this.lastSelectionOrigin = e, this.lastSelectionTime = Date.now();
  }
  constructor(e) {
    this.view = e, this.lastKeyCode = 0, this.lastKeyTime = 0, this.lastTouchTime = 0, this.lastFocusTime = 0, this.lastScrollTop = 0, this.lastScrollLeft = 0, this.pendingIOSKey = void 0, this.tabFocusMode = -1, this.lastSelectionOrigin = null, this.lastSelectionTime = 0, this.lastContextMenu = 0, this.scrollHandlers = [], this.handlers = /* @__PURE__ */ Object.create(null), this.composing = -1, this.compositionFirstChange = null, this.compositionEndedAt = 0, this.compositionPendingKey = !1, this.compositionPendingChange = !1, this.insertingText = "", this.insertingTextAt = 0, this.mouseSelection = null, this.draggedContent = null, this.handleEvent = this.handleEvent.bind(this), this.notifiedFocused = e.hasFocus, j.safari && e.contentDOM.addEventListener("input", () => null), j.gecko && u0(e.contentDOM.ownerDocument);
  }
  handleEvent(e) {
    !s0(this.view, e) || this.ignoreDuringComposition(e) || e.type == "keydown" && this.keydown(e) || (this.view.updateState != 0 ? Promise.resolve().then(() => this.runHandlers(e.type, e)) : this.runHandlers(e.type, e));
  }
  runHandlers(e, t) {
    let i = this.handlers[e];
    if (i) {
      for (let r of i.observers)
        r(this.view, t);
      for (let r of i.handlers) {
        if (t.defaultPrevented)
          break;
        if (r(this.view, t)) {
          t.preventDefault();
          break;
        }
      }
    }
  }
  ensureHandlers(e) {
    let t = Zm(e), i = this.handlers, r = this.view.contentDOM;
    for (let s in t)
      if (s != "scroll") {
        let o = !t[s].handlers.length, l = i[s];
        l && o != !l.handlers.length && (r.removeEventListener(s, this.handleEvent), l = null), l || r.addEventListener(s, this.handleEvent, { passive: o });
      }
    for (let s in i)
      s != "scroll" && !t[s] && r.removeEventListener(s, this.handleEvent);
    this.handlers = t;
  }
  keydown(e) {
    if (this.lastKeyCode = e.keyCode, this.lastKeyTime = Date.now(), e.keyCode == 9 && this.tabFocusMode > -1 && (!this.tabFocusMode || Date.now() <= this.tabFocusMode))
      return !0;
    if (this.tabFocusMode > 0 && e.keyCode != 27 && Su.indexOf(e.keyCode) < 0 && (this.tabFocusMode = -1), j.android && j.chrome && !e.synthetic && (e.keyCode == 13 || e.keyCode == 8))
      return this.view.observer.delayAndroidKey(e.key, e.keyCode), !0;
    let t;
    return j.ios && !e.synthetic && !e.altKey && !e.metaKey && ((t = wu.find((i) => i.keyCode == e.keyCode)) && !e.ctrlKey || Jm.indexOf(e.key) > -1 && e.ctrlKey && !e.shiftKey) ? (this.pendingIOSKey = t || e, setTimeout(() => this.flushIOSKey(), 250), !0) : (e.keyCode != 229 && this.view.observer.forceFlush(), !1);
  }
  flushIOSKey(e) {
    let t = this.pendingIOSKey;
    return !t || t.key == "Enter" && e && e.from < e.to && /^\S+$/.test(e.insert.toString()) ? !1 : (this.pendingIOSKey = void 0, fn(this.view.contentDOM, t.key, t.keyCode, t instanceof KeyboardEvent ? t : void 0));
  }
  ignoreDuringComposition(e) {
    return !/^key/.test(e.type) || e.synthetic ? !1 : this.composing > 0 ? !0 : j.safari && !j.ios && this.compositionPendingKey && Date.now() - this.compositionEndedAt < 100 ? (this.compositionPendingKey = !1, !0) : !1;
  }
  startMouseSelection(e) {
    this.mouseSelection && this.mouseSelection.destroy(), this.mouseSelection = e;
  }
  update(e) {
    this.view.observer.update(e), this.mouseSelection && this.mouseSelection.update(e), this.draggedContent && e.docChanged && (this.draggedContent = this.draggedContent.map(e.changes)), e.transactions.length && (this.lastKeyCode = this.lastSelectionTime = 0);
  }
  destroy() {
    this.mouseSelection && this.mouseSelection.destroy();
  }
}
function Bh(n, e) {
  return (t, i) => {
    try {
      return e.call(n, i, t);
    } catch (r) {
      ft(t.state, r);
    }
  };
}
function Zm(n) {
  let e = /* @__PURE__ */ Object.create(null);
  function t(i) {
    return e[i] || (e[i] = { observers: [], handlers: [] });
  }
  for (let i of n) {
    let r = i.spec, s = r && r.plugin.domEventHandlers, o = r && r.plugin.domEventObservers;
    if (s)
      for (let l in s) {
        let a = s[l];
        a && t(l).handlers.push(Bh(i.value, a));
      }
    if (o)
      for (let l in o) {
        let a = o[l];
        a && t(l).observers.push(Bh(i.value, a));
      }
  }
  for (let i in Ft)
    t(i).handlers.push(Ft[i]);
  for (let i in Lt)
    t(i).observers.push(Lt[i]);
  return e;
}
const wu = [
  { key: "Backspace", keyCode: 8, inputType: "deleteContentBackward" },
  { key: "Enter", keyCode: 13, inputType: "insertParagraph" },
  { key: "Enter", keyCode: 13, inputType: "insertLineBreak" },
  { key: "Delete", keyCode: 46, inputType: "deleteContentForward" }
], Jm = "dthko", Su = [16, 17, 18, 20, 91, 92, 224, 225], Or = 6;
function Ar(n) {
  return Math.max(0, n) * 0.7 + 8;
}
function e0(n, e) {
  return Math.max(Math.abs(n.clientX - e.clientX), Math.abs(n.clientY - e.clientY));
}
class t0 {
  constructor(e, t, i, r) {
    this.view = e, this.startEvent = t, this.style = i, this.mustSelect = r, this.scrollSpeed = { x: 0, y: 0 }, this.scrolling = -1, this.lastEvent = t, this.scrollParents = rm(e.contentDOM), this.atoms = e.state.facet(ur).map((o) => o(e));
    let s = e.contentDOM.ownerDocument;
    s.addEventListener("mousemove", this.move = this.move.bind(this)), s.addEventListener("mouseup", this.up = this.up.bind(this)), this.extend = t.shiftKey, this.multiple = e.state.facet(pe.allowMultipleSelections) && i0(e, t), this.dragging = r0(e, t) && Au(t) == 1 ? null : !1;
  }
  start(e) {
    this.dragging === !1 && this.select(e);
  }
  move(e) {
    if (e.buttons == 0)
      return this.destroy();
    if (this.dragging || this.dragging == null && e0(this.startEvent, e) < 10)
      return;
    this.select(this.lastEvent = e);
    let t = 0, i = 0, r = 0, s = 0, o = this.view.win.innerWidth, l = this.view.win.innerHeight;
    this.scrollParents.x && ({ left: r, right: o } = this.scrollParents.x.getBoundingClientRect()), this.scrollParents.y && ({ top: s, bottom: l } = this.scrollParents.y.getBoundingClientRect());
    let a = Gl(this.view);
    e.clientX - a.left <= r + Or ? t = -Ar(r - e.clientX) : e.clientX + a.right >= o - Or && (t = Ar(e.clientX - o)), e.clientY - a.top <= s + Or ? i = -Ar(s - e.clientY) : e.clientY + a.bottom >= l - Or && (i = Ar(e.clientY - l)), this.setScrollSpeed(t, i);
  }
  up(e) {
    this.dragging == null && this.select(this.lastEvent), this.dragging || e.preventDefault(), this.destroy();
  }
  destroy() {
    this.setScrollSpeed(0, 0);
    let e = this.view.contentDOM.ownerDocument;
    e.removeEventListener("mousemove", this.move), e.removeEventListener("mouseup", this.up), this.view.inputState.mouseSelection = this.view.inputState.draggedContent = null;
  }
  setScrollSpeed(e, t) {
    this.scrollSpeed = { x: e, y: t }, e || t ? this.scrolling < 0 && (this.scrolling = setInterval(() => this.scroll(), 50)) : this.scrolling > -1 && (clearInterval(this.scrolling), this.scrolling = -1);
  }
  scroll() {
    let { x: e, y: t } = this.scrollSpeed;
    e && this.scrollParents.x && (this.scrollParents.x.scrollLeft += e, e = 0), t && this.scrollParents.y && (this.scrollParents.y.scrollTop += t, t = 0), (e || t) && this.view.win.scrollBy(e, t), this.dragging === !1 && this.select(this.lastEvent);
  }
  select(e) {
    let { view: t } = this, i = vu(this.atoms, this.style.get(e, this.extend, this.multiple));
    (this.mustSelect || !i.eq(t.state.selection, this.dragging === !1)) && this.view.dispatch({
      selection: i,
      userEvent: "select.pointer"
    }), this.mustSelect = !1;
  }
  update(e) {
    e.transactions.some((t) => t.isUserEvent("input.type")) ? this.destroy() : this.style.update(e) && setTimeout(() => this.select(this.lastEvent), 20);
  }
}
function i0(n, e) {
  let t = n.state.facet(tu);
  return t.length ? t[0](e) : j.mac ? e.metaKey : e.ctrlKey;
}
function n0(n, e) {
  let t = n.state.facet(iu);
  return t.length ? t[0](e) : j.mac ? !e.altKey : !e.ctrlKey;
}
function r0(n, e) {
  let { main: t } = n.state.selection;
  if (t.empty)
    return !1;
  let i = yn(n.root);
  if (!i || i.rangeCount == 0)
    return !0;
  let r = i.getRangeAt(0).getClientRects();
  for (let s = 0; s < r.length; s++) {
    let o = r[s];
    if (o.left <= e.clientX && o.right >= e.clientX && o.top <= e.clientY && o.bottom >= e.clientY)
      return !0;
  }
  return !1;
}
function s0(n, e) {
  if (!e.bubbles)
    return !0;
  if (e.defaultPrevented)
    return !1;
  for (let t = e.target, i; t != n.contentDOM; t = t.parentNode)
    if (!t || t.nodeType == 11 || (i = Ne.get(t)) && i.isWidget() && !i.isHidden && i.widget.ignoreEvent(e))
      return !1;
  return !0;
}
const Ft = /* @__PURE__ */ Object.create(null), Lt = /* @__PURE__ */ Object.create(null), Cu = j.ie && j.ie_version < 15 || j.ios && j.webkit_version < 604;
function o0(n) {
  let e = n.dom.parentNode;
  if (!e)
    return;
  let t = e.appendChild(document.createElement("textarea"));
  t.style.cssText = "position: fixed; left: -10000px; top: 10px", t.focus(), setTimeout(() => {
    n.focus(), t.remove(), Ou(n, t.value);
  }, 50);
}
function Fs(n, e, t) {
  for (let i of n.facet(e))
    t = i(t, n);
  return t;
}
function Ou(n, e) {
  e = Fs(n.state, jl, e);
  let { state: t } = n, i, r = 1, s = t.toText(e), o = s.lines == t.selection.ranges.length;
  if (ul != null && t.selection.ranges.every((a) => a.empty) && ul == s.toString()) {
    let a = -1;
    i = t.changeByRange((f) => {
      let u = t.doc.lineAt(f.from);
      if (u.from == a)
        return { range: f };
      a = u.from;
      let g = t.toText((o ? s.line(r++).text : e) + t.lineBreak);
      return {
        changes: { from: u.from, insert: g },
        range: E.cursor(f.from + g.length)
      };
    });
  } else o ? i = t.changeByRange((a) => {
    let f = s.line(r++);
    return {
      changes: { from: a.from, to: a.to, insert: f.text },
      range: E.cursor(a.from + f.length)
    };
  }) : i = t.replaceSelection(s);
  n.dispatch(i, {
    userEvent: "input.paste",
    scrollIntoView: !0
  });
}
Lt.scroll = (n) => {
  n.inputState.lastScrollTop = n.scrollDOM.scrollTop, n.inputState.lastScrollLeft = n.scrollDOM.scrollLeft;
};
Ft.keydown = (n, e) => (n.inputState.setSelectionOrigin("select"), e.keyCode == 27 && n.inputState.tabFocusMode != 0 && (n.inputState.tabFocusMode = Date.now() + 2e3), !1);
Lt.touchstart = (n, e) => {
  n.inputState.lastTouchTime = Date.now(), n.inputState.setSelectionOrigin("select.pointer");
};
Lt.touchmove = (n) => {
  n.inputState.setSelectionOrigin("select.pointer");
};
Ft.mousedown = (n, e) => {
  if (n.observer.flush(), n.inputState.lastTouchTime > Date.now() - 2e3)
    return !1;
  let t = null;
  for (let i of n.state.facet(nu))
    if (t = i(n, e), t)
      break;
  if (!t && e.button == 0 && (t = a0(n, e)), t) {
    let i = !n.hasFocus;
    n.inputState.startMouseSelection(new t0(n, e, t, i)), i && n.observer.ignore(() => {
      _f(n.contentDOM);
      let s = n.root.activeElement;
      s && !s.contains(n.contentDOM) && s.blur();
    });
    let r = n.inputState.mouseSelection;
    if (r)
      return r.start(e), r.dragging === !1;
  } else
    n.inputState.setSelectionOrigin("select.pointer");
  return !1;
};
function Eh(n, e, t, i) {
  if (i == 1)
    return E.cursor(e, t);
  if (i == 2)
    return Qm(n.state, e, t);
  {
    let r = n.docView.lineAt(e, t), s = n.state.doc.lineAt(r ? r.posAtEnd : e), o = r ? r.posAtStart : s.from, l = r ? r.posAtEnd : s.to;
    return l < n.state.doc.length && l == s.to && l++, E.range(o, l);
  }
}
const l0 = j.ie && j.ie_version <= 11;
let Ih = null, Nh = 0, Fh = 0;
function Au(n) {
  if (!l0)
    return n.detail;
  let e = Ih, t = Fh;
  return Ih = n, Fh = Date.now(), Nh = !e || t > Date.now() - 400 && Math.abs(e.clientX - n.clientX) < 2 && Math.abs(e.clientY - n.clientY) < 2 ? (Nh + 1) % 3 : 1;
}
function a0(n, e) {
  let t = n.posAndSideAtCoords({ x: e.clientX, y: e.clientY }, !1), i = Au(e), r = n.state.selection;
  return {
    update(s) {
      s.docChanged && (t.pos = s.changes.mapPos(t.pos), r = r.map(s.changes));
    },
    get(s, o, l) {
      let a = n.posAndSideAtCoords({ x: s.clientX, y: s.clientY }, !1), f, u = Eh(n, a.pos, a.assoc, i);
      if (t.pos != a.pos && !o) {
        let g = Eh(n, t.pos, t.assoc, i), y = Math.min(g.from, u.from), b = Math.max(g.to, u.to);
        u = y < u.from ? E.range(y, b) : E.range(b, y);
      }
      return o ? r.replaceRange(r.main.extend(u.from, u.to)) : l && i == 1 && r.ranges.length > 1 && (f = h0(r, a.pos)) ? f : l ? r.addRange(u) : E.create([u]);
    }
  };
}
function h0(n, e) {
  for (let t = 0; t < n.ranges.length; t++) {
    let { from: i, to: r } = n.ranges[t];
    if (i <= e && r >= e)
      return E.create(n.ranges.slice(0, t).concat(n.ranges.slice(t + 1)), n.mainIndex == t ? 0 : n.mainIndex - (n.mainIndex > t ? 1 : 0));
  }
  return null;
}
Ft.dragstart = (n, e) => {
  let { selection: { main: t } } = n.state;
  if (e.target.draggable) {
    let r = n.docView.tile.nearest(e.target);
    if (r && r.isWidget()) {
      let s = r.posAtStart, o = s + r.length;
      (s >= t.to || o <= t.from) && (t = E.range(s, o));
    }
  }
  let { inputState: i } = n;
  return i.mouseSelection && (i.mouseSelection.dragging = !0), i.draggedContent = t, e.dataTransfer && (e.dataTransfer.setData("Text", Fs(n.state, Ul, n.state.sliceDoc(t.from, t.to))), e.dataTransfer.effectAllowed = "copyMove"), !1;
};
Ft.dragend = (n) => (n.inputState.draggedContent = null, !1);
function Wh(n, e, t, i) {
  if (t = Fs(n.state, jl, t), !t)
    return;
  let r = n.posAtCoords({ x: e.clientX, y: e.clientY }, !1), { draggedContent: s } = n.inputState, o = i && s && n0(n, e) ? { from: s.from, to: s.to } : null, l = { from: r, insert: t }, a = n.state.changes(o ? [o, l] : l);
  n.focus(), n.dispatch({
    changes: a,
    selection: { anchor: a.mapPos(r, -1), head: a.mapPos(r, 1) },
    userEvent: o ? "move.drop" : "input.drop"
  }), n.inputState.draggedContent = null;
}
Ft.drop = (n, e) => {
  if (!e.dataTransfer)
    return !1;
  if (n.state.readOnly)
    return !0;
  let t = e.dataTransfer.files;
  if (t && t.length) {
    let i = Array(t.length), r = 0, s = () => {
      ++r == t.length && Wh(n, e, i.filter((o) => o != null).join(n.state.lineBreak), !1);
    };
    for (let o = 0; o < t.length; o++) {
      let l = new FileReader();
      l.onerror = s, l.onload = () => {
        /[\x00-\x08\x0e-\x1f]{2}/.test(l.result) || (i[o] = l.result), s();
      }, l.readAsText(t[o]);
    }
    return !0;
  } else {
    let i = e.dataTransfer.getData("Text");
    if (i)
      return Wh(n, e, i, !0), !0;
  }
  return !1;
};
Ft.paste = (n, e) => {
  if (n.state.readOnly)
    return !0;
  n.observer.flush();
  let t = Cu ? null : e.clipboardData;
  return t ? (Ou(n, t.getData("text/plain") || t.getData("text/uri-list")), !0) : (o0(n), !1);
};
function c0(n, e) {
  let t = n.dom.parentNode;
  if (!t)
    return;
  let i = t.appendChild(document.createElement("textarea"));
  i.style.cssText = "position: fixed; left: -10000px; top: 10px", i.value = e, i.focus(), i.selectionEnd = e.length, i.selectionStart = 0, setTimeout(() => {
    i.remove(), n.focus();
  }, 50);
}
function f0(n) {
  let e = [], t = [], i = !1;
  for (let r of n.selection.ranges)
    r.empty || (e.push(n.sliceDoc(r.from, r.to)), t.push(r));
  if (!e.length) {
    let r = -1;
    for (let { from: s } of n.selection.ranges) {
      let o = n.doc.lineAt(s);
      o.number > r && (e.push(o.text), t.push({ from: o.from, to: Math.min(n.doc.length, o.to + 1) })), r = o.number;
    }
    i = !0;
  }
  return { text: Fs(n, Ul, e.join(n.lineBreak)), ranges: t, linewise: i };
}
let ul = null;
Ft.copy = Ft.cut = (n, e) => {
  let t = yn(n.root);
  if (t && !Kn(n.contentDOM, t))
    return !1;
  let { text: i, ranges: r, linewise: s } = f0(n.state);
  if (!i && !s)
    return !1;
  ul = s ? i : null, e.type == "cut" && !n.state.readOnly && n.dispatch({
    changes: r,
    scrollIntoView: !0,
    userEvent: "delete.cut"
  });
  let o = Cu ? null : e.clipboardData;
  return o ? (o.clearData(), o.setData("text/plain", i), !0) : (c0(n, i), !1);
};
const Mu = /* @__PURE__ */ hi.define();
function Tu(n, e) {
  let t = [];
  for (let i of n.facet(ou)) {
    let r = i(n, e);
    r && t.push(r);
  }
  return t.length ? n.update({ effects: t, annotations: Mu.of(!0) }) : null;
}
function Pu(n) {
  setTimeout(() => {
    let e = n.hasFocus;
    if (e != n.inputState.notifiedFocused) {
      let t = Tu(n.state, e);
      t ? n.dispatch(t) : n.update([]);
    }
  }, 10);
}
Lt.focus = (n) => {
  n.inputState.lastFocusTime = Date.now(), !n.scrollDOM.scrollTop && (n.inputState.lastScrollTop || n.inputState.lastScrollLeft) && (n.scrollDOM.scrollTop = n.inputState.lastScrollTop, n.scrollDOM.scrollLeft = n.inputState.lastScrollLeft), Pu(n);
};
Lt.blur = (n) => {
  n.observer.clearSelectionRange(), Pu(n);
};
Lt.compositionstart = Lt.compositionupdate = (n) => {
  n.observer.editContext || (n.inputState.compositionFirstChange == null && (n.inputState.compositionFirstChange = !0), n.inputState.composing < 0 && (n.inputState.composing = 0));
};
Lt.compositionend = (n) => {
  n.observer.editContext || (n.inputState.composing = -1, n.inputState.compositionEndedAt = Date.now(), n.inputState.compositionPendingKey = !0, n.inputState.compositionPendingChange = n.observer.pendingRecords().length > 0, n.inputState.compositionFirstChange = null, j.chrome && j.android ? n.observer.flushSoon() : n.inputState.compositionPendingChange ? Promise.resolve().then(() => n.observer.flush()) : setTimeout(() => {
    n.inputState.composing < 0 && n.docView.hasComposition && n.update([]);
  }, 50));
};
Lt.contextmenu = (n) => {
  n.inputState.lastContextMenu = Date.now();
};
Ft.beforeinput = (n, e) => {
  var t, i;
  if ((e.inputType == "insertText" || e.inputType == "insertCompositionText") && (n.inputState.insertingText = e.data, n.inputState.insertingTextAt = Date.now()), e.inputType == "insertReplacementText" && n.observer.editContext) {
    let s = (t = e.dataTransfer) === null || t === void 0 ? void 0 : t.getData("text/plain"), o = e.getTargetRanges();
    if (s && o.length) {
      let l = o[0], a = n.posAtDOM(l.startContainer, l.startOffset), f = n.posAtDOM(l.endContainer, l.endOffset);
      return Zl(n, { from: a, to: f, insert: n.state.toText(s) }, null), !0;
    }
  }
  let r;
  if (j.chrome && j.android && (r = wu.find((s) => s.inputType == e.inputType)) && (n.observer.delayAndroidKey(r.key, r.keyCode), r.key == "Backspace" || r.key == "Delete")) {
    let s = ((i = window.visualViewport) === null || i === void 0 ? void 0 : i.height) || 0;
    setTimeout(() => {
      var o;
      (((o = window.visualViewport) === null || o === void 0 ? void 0 : o.height) || 0) > s + 10 && n.hasFocus && (n.contentDOM.blur(), n.focus());
    }, 100);
  }
  return j.ios && e.inputType == "deleteContentForward" && n.observer.flushSoon(), j.safari && e.inputType == "insertText" && n.inputState.composing >= 0 && setTimeout(() => Lt.compositionend(n, e), 20), !1;
};
const Qh = /* @__PURE__ */ new Set();
function u0(n) {
  Qh.has(n) || (Qh.add(n), n.addEventListener("copy", () => {
  }), n.addEventListener("cut", () => {
  }));
}
const Vh = ["pre-wrap", "normal", "pre-line", "break-spaces"];
let kn = !1;
function Hh() {
  kn = !1;
}
class d0 {
  constructor(e) {
    this.lineWrapping = e, this.doc = ge.empty, this.heightSamples = {}, this.lineHeight = 14, this.charWidth = 7, this.textHeight = 14, this.lineLength = 30;
  }
  heightForGap(e, t) {
    let i = this.doc.lineAt(t).number - this.doc.lineAt(e).number + 1;
    return this.lineWrapping && (i += Math.max(0, Math.ceil((t - e - i * this.lineLength * 0.5) / this.lineLength))), this.lineHeight * i;
  }
  heightForLine(e) {
    return this.lineWrapping ? (1 + Math.max(0, Math.ceil((e - this.lineLength) / Math.max(1, this.lineLength - 5)))) * this.lineHeight : this.lineHeight;
  }
  setDoc(e) {
    return this.doc = e, this;
  }
  mustRefreshForWrapping(e) {
    return Vh.indexOf(e) > -1 != this.lineWrapping;
  }
  mustRefreshForHeights(e) {
    let t = !1;
    for (let i = 0; i < e.length; i++) {
      let r = e[i];
      r < 0 ? i++ : this.heightSamples[Math.floor(r * 10)] || (t = !0, this.heightSamples[Math.floor(r * 10)] = !0);
    }
    return t;
  }
  refresh(e, t, i, r, s, o) {
    let l = Vh.indexOf(e) > -1, a = Math.abs(t - this.lineHeight) > 0.3 || this.lineWrapping != l || Math.abs(i - this.charWidth) > 0.1;
    if (this.lineWrapping = l, this.lineHeight = t, this.charWidth = i, this.textHeight = r, this.lineLength = s, a) {
      this.heightSamples = {};
      for (let f = 0; f < o.length; f++) {
        let u = o[f];
        u < 0 ? f++ : this.heightSamples[Math.floor(u * 10)] = !0;
      }
    }
    return a;
  }
}
class p0 {
  constructor(e, t) {
    this.from = e, this.heights = t, this.index = 0;
  }
  get more() {
    return this.index < this.heights.length;
  }
}
class It {
  /**
  @internal
  */
  constructor(e, t, i, r, s) {
    this.from = e, this.length = t, this.top = i, this.height = r, this._content = s;
  }
  /**
  The type of element this is. When querying lines, this may be
  an array of all the blocks that make up the line.
  */
  get type() {
    return typeof this._content == "number" ? Ye.Text : Array.isArray(this._content) ? this._content : this._content.type;
  }
  /**
  The end of the element as a document position.
  */
  get to() {
    return this.from + this.length;
  }
  /**
  The bottom position of the element.
  */
  get bottom() {
    return this.top + this.height;
  }
  /**
  If this is a widget block, this will return the widget
  associated with it.
  */
  get widget() {
    return this._content instanceof qi ? this._content.widget : null;
  }
  /**
  If this is a textblock, this holds the number of line breaks
  that appear in widgets inside the block.
  */
  get widgetLineBreaks() {
    return typeof this._content == "number" ? this._content : 0;
  }
  /**
  @internal
  */
  join(e) {
    let t = (Array.isArray(this._content) ? this._content : [this]).concat(Array.isArray(e._content) ? e._content : [e]);
    return new It(this.from, this.length + e.length, this.top, this.height + e.height, t);
  }
}
var Oe = /* @__PURE__ */ (function(n) {
  return n[n.ByPos = 0] = "ByPos", n[n.ByHeight = 1] = "ByHeight", n[n.ByPosNoHeight = 2] = "ByPosNoHeight", n;
})(Oe || (Oe = {}));
const Ur = 1e-3;
class st {
  constructor(e, t, i = 2) {
    this.length = e, this.height = t, this.flags = i;
  }
  get outdated() {
    return (this.flags & 2) > 0;
  }
  set outdated(e) {
    this.flags = (e ? 2 : 0) | this.flags & -3;
  }
  setHeight(e) {
    this.height != e && (Math.abs(this.height - e) > Ur && (kn = !0), this.height = e);
  }
  // Base case is to replace a leaf node, which simply builds a tree
  // from the new nodes and returns that (HeightMapBranch and
  // HeightMapGap override this to actually use from/to)
  replace(e, t, i) {
    return st.of(i);
  }
  // Again, these are base cases, and are overridden for branch and gap nodes.
  decomposeLeft(e, t) {
    t.push(this);
  }
  decomposeRight(e, t) {
    t.push(this);
  }
  applyChanges(e, t, i, r) {
    let s = this, o = i.doc;
    for (let l = r.length - 1; l >= 0; l--) {
      let { fromA: a, toA: f, fromB: u, toB: g } = r[l], y = s.lineAt(a, Oe.ByPosNoHeight, i.setDoc(t), 0, 0), b = y.to >= f ? y : s.lineAt(f, Oe.ByPosNoHeight, i, 0, 0);
      for (g += b.to - f, f = b.to; l > 0 && y.from <= r[l - 1].toA; )
        a = r[l - 1].fromA, u = r[l - 1].fromB, l--, a < y.from && (y = s.lineAt(a, Oe.ByPosNoHeight, i, 0, 0));
      u += y.from - a, a = y.from;
      let k = Jl.build(i.setDoc(o), e, u, g);
      s = cs(s, s.replace(a, f, k));
    }
    return s.updateHeight(i, 0);
  }
  static empty() {
    return new vt(0, 0, 0);
  }
  // nodes uses null values to indicate the position of line breaks.
  // There are never line breaks at the start or end of the array, or
  // two line breaks next to each other, and the array isn't allowed
  // to be empty (same restrictions as return value from the builder).
  static of(e) {
    if (e.length == 1)
      return e[0];
    let t = 0, i = e.length, r = 0, s = 0;
    for (; ; )
      if (t == i)
        if (r > s * 2) {
          let l = e[t - 1];
          l.break ? e.splice(--t, 1, l.left, null, l.right) : e.splice(--t, 1, l.left, l.right), i += 1 + l.break, r -= l.size;
        } else if (s > r * 2) {
          let l = e[i];
          l.break ? e.splice(i, 1, l.left, null, l.right) : e.splice(i, 1, l.left, l.right), i += 2 + l.break, s -= l.size;
        } else
          break;
      else if (r < s) {
        let l = e[t++];
        l && (r += l.size);
      } else {
        let l = e[--i];
        l && (s += l.size);
      }
    let o = 0;
    return e[t - 1] == null ? (o = 1, t--) : e[t] == null && (o = 1, i++), new m0(st.of(e.slice(0, t)), o, st.of(e.slice(i)));
  }
}
function cs(n, e) {
  return n == e ? n : (n.constructor != e.constructor && (kn = !0), e);
}
st.prototype.size = 1;
const g0 = /* @__PURE__ */ G.replace({});
class Ru extends st {
  constructor(e, t, i) {
    super(e, t), this.deco = i, this.spaceAbove = 0;
  }
  mainBlock(e, t) {
    return new It(t, this.length, e + this.spaceAbove, this.height - this.spaceAbove, this.deco || 0);
  }
  blockAt(e, t, i, r) {
    return this.spaceAbove && e < i + this.spaceAbove ? new It(r, 0, i, this.spaceAbove, g0) : this.mainBlock(i, r);
  }
  lineAt(e, t, i, r, s) {
    let o = this.mainBlock(r, s);
    return this.spaceAbove ? this.blockAt(0, i, r, s).join(o) : o;
  }
  forEachLine(e, t, i, r, s, o) {
    e <= s + this.length && t >= s && o(this.lineAt(0, Oe.ByPos, i, r, s));
  }
  setMeasuredHeight(e) {
    let t = e.heights[e.index++];
    t < 0 ? (this.spaceAbove = -t, t = e.heights[e.index++]) : this.spaceAbove = 0, this.setHeight(t);
  }
  updateHeight(e, t = 0, i = !1, r) {
    return r && r.from <= t && r.more && this.setMeasuredHeight(r), this.outdated = !1, this;
  }
  toString() {
    return `block(${this.length})`;
  }
}
class vt extends Ru {
  constructor(e, t, i) {
    super(e, t, null), this.collapsed = 0, this.widgetHeight = 0, this.breaks = 0, this.spaceAbove = i;
  }
  mainBlock(e, t) {
    return new It(t, this.length, e + this.spaceAbove, this.height - this.spaceAbove, this.breaks);
  }
  replace(e, t, i) {
    let r = i[0];
    return i.length == 1 && (r instanceof vt || r instanceof je && r.flags & 4) && Math.abs(this.length - r.length) < 10 ? (r instanceof je ? r = new vt(r.length, this.height, this.spaceAbove) : r.height = this.height, this.outdated || (r.outdated = !1), r) : st.of(i);
  }
  updateHeight(e, t = 0, i = !1, r) {
    return r && r.from <= t && r.more ? this.setMeasuredHeight(r) : (i || this.outdated) && (this.spaceAbove = 0, this.setHeight(Math.max(this.widgetHeight, e.heightForLine(this.length - this.collapsed)) + this.breaks * e.lineHeight)), this.outdated = !1, this;
  }
  toString() {
    return `line(${this.length}${this.collapsed ? -this.collapsed : ""}${this.widgetHeight ? ":" + this.widgetHeight : ""})`;
  }
}
class je extends st {
  constructor(e) {
    super(e, 0);
  }
  heightMetrics(e, t) {
    let i = e.doc.lineAt(t).number, r = e.doc.lineAt(t + this.length).number, s = r - i + 1, o, l = 0;
    if (e.lineWrapping) {
      let a = Math.min(this.height, e.lineHeight * s);
      o = a / s, this.length > s + 1 && (l = (this.height - a) / (this.length - s - 1));
    } else
      o = this.height / s;
    return { firstLine: i, lastLine: r, perLine: o, perChar: l };
  }
  blockAt(e, t, i, r) {
    let { firstLine: s, lastLine: o, perLine: l, perChar: a } = this.heightMetrics(t, r);
    if (t.lineWrapping) {
      let f = r + (e < t.lineHeight ? 0 : Math.round(Math.max(0, Math.min(1, (e - i) / this.height)) * this.length)), u = t.doc.lineAt(f), g = l + u.length * a, y = Math.max(i, e - g / 2);
      return new It(u.from, u.length, y, g, 0);
    } else {
      let f = Math.max(0, Math.min(o - s, Math.floor((e - i) / l))), { from: u, length: g } = t.doc.line(s + f);
      return new It(u, g, i + l * f, l, 0);
    }
  }
  lineAt(e, t, i, r, s) {
    if (t == Oe.ByHeight)
      return this.blockAt(e, i, r, s);
    if (t == Oe.ByPosNoHeight) {
      let { from: b, to: k } = i.doc.lineAt(e);
      return new It(b, k - b, 0, 0, 0);
    }
    let { firstLine: o, perLine: l, perChar: a } = this.heightMetrics(i, s), f = i.doc.lineAt(e), u = l + f.length * a, g = f.number - o, y = r + l * g + a * (f.from - s - g);
    return new It(f.from, f.length, Math.max(r, Math.min(y, r + this.height - u)), u, 0);
  }
  forEachLine(e, t, i, r, s, o) {
    e = Math.max(e, s), t = Math.min(t, s + this.length);
    let { firstLine: l, perLine: a, perChar: f } = this.heightMetrics(i, s);
    for (let u = e, g = r; u <= t; ) {
      let y = i.doc.lineAt(u);
      if (u == e) {
        let k = y.number - l;
        g += a * k + f * (e - s - k);
      }
      let b = a + f * y.length;
      o(new It(y.from, y.length, g, b, 0)), g += b, u = y.to + 1;
    }
  }
  replace(e, t, i) {
    let r = this.length - t;
    if (r > 0) {
      let s = i[i.length - 1];
      s instanceof je ? i[i.length - 1] = new je(s.length + r) : i.push(null, new je(r - 1));
    }
    if (e > 0) {
      let s = i[0];
      s instanceof je ? i[0] = new je(e + s.length) : i.unshift(new je(e - 1), null);
    }
    return st.of(i);
  }
  decomposeLeft(e, t) {
    t.push(new je(e - 1), null);
  }
  decomposeRight(e, t) {
    t.push(null, new je(this.length - e - 1));
  }
  updateHeight(e, t = 0, i = !1, r) {
    let s = t + this.length;
    if (r && r.from <= t + this.length && r.more) {
      let o = [], l = Math.max(t, r.from), a = -1;
      for (r.from > t && o.push(new je(r.from - t - 1).updateHeight(e, t)); l <= s && r.more; ) {
        let u = e.doc.lineAt(l).length;
        o.length && o.push(null);
        let g = r.heights[r.index++], y = 0;
        g < 0 && (y = -g, g = r.heights[r.index++]), a == -1 ? a = g : Math.abs(g - a) >= Ur && (a = -2);
        let b = new vt(u, g, y);
        b.outdated = !1, o.push(b), l += u + 1;
      }
      l <= s && o.push(null, new je(s - l).updateHeight(e, l));
      let f = st.of(o);
      return (a < 0 || Math.abs(f.height - this.height) >= Ur || Math.abs(a - this.heightMetrics(e, t).perLine) >= Ur) && (kn = !0), cs(this, f);
    } else (i || this.outdated) && (this.setHeight(e.heightForGap(t, t + this.length)), this.outdated = !1);
    return this;
  }
  toString() {
    return `gap(${this.length})`;
  }
}
class m0 extends st {
  constructor(e, t, i) {
    super(e.length + t + i.length, e.height + i.height, t | (e.outdated || i.outdated ? 2 : 0)), this.left = e, this.right = i, this.size = e.size + i.size;
  }
  get break() {
    return this.flags & 1;
  }
  blockAt(e, t, i, r) {
    let s = i + this.left.height;
    return e < s ? this.left.blockAt(e, t, i, r) : this.right.blockAt(e, t, s, r + this.left.length + this.break);
  }
  lineAt(e, t, i, r, s) {
    let o = r + this.left.height, l = s + this.left.length + this.break, a = t == Oe.ByHeight ? e < o : e < l, f = a ? this.left.lineAt(e, t, i, r, s) : this.right.lineAt(e, t, i, o, l);
    if (this.break || (a ? f.to < l : f.from > l))
      return f;
    let u = t == Oe.ByPosNoHeight ? Oe.ByPosNoHeight : Oe.ByPos;
    return a ? f.join(this.right.lineAt(l, u, i, o, l)) : this.left.lineAt(l, u, i, r, s).join(f);
  }
  forEachLine(e, t, i, r, s, o) {
    let l = r + this.left.height, a = s + this.left.length + this.break;
    if (this.break)
      e < a && this.left.forEachLine(e, t, i, r, s, o), t >= a && this.right.forEachLine(e, t, i, l, a, o);
    else {
      let f = this.lineAt(a, Oe.ByPos, i, r, s);
      e < f.from && this.left.forEachLine(e, f.from - 1, i, r, s, o), f.to >= e && f.from <= t && o(f), t > f.to && this.right.forEachLine(f.to + 1, t, i, l, a, o);
    }
  }
  replace(e, t, i) {
    let r = this.left.length + this.break;
    if (t < r)
      return this.balanced(this.left.replace(e, t, i), this.right);
    if (e > this.left.length)
      return this.balanced(this.left, this.right.replace(e - r, t - r, i));
    let s = [];
    e > 0 && this.decomposeLeft(e, s);
    let o = s.length;
    for (let l of i)
      s.push(l);
    if (e > 0 && zh(s, o - 1), t < this.length) {
      let l = s.length;
      this.decomposeRight(t, s), zh(s, l);
    }
    return st.of(s);
  }
  decomposeLeft(e, t) {
    let i = this.left.length;
    if (e <= i)
      return this.left.decomposeLeft(e, t);
    t.push(this.left), this.break && (i++, e >= i && t.push(null)), e > i && this.right.decomposeLeft(e - i, t);
  }
  decomposeRight(e, t) {
    let i = this.left.length, r = i + this.break;
    if (e >= r)
      return this.right.decomposeRight(e - r, t);
    e < i && this.left.decomposeRight(e, t), this.break && e < r && t.push(null), t.push(this.right);
  }
  balanced(e, t) {
    return e.size > 2 * t.size || t.size > 2 * e.size ? st.of(this.break ? [e, null, t] : [e, t]) : (this.left = cs(this.left, e), this.right = cs(this.right, t), this.setHeight(e.height + t.height), this.outdated = e.outdated || t.outdated, this.size = e.size + t.size, this.length = e.length + this.break + t.length, this);
  }
  updateHeight(e, t = 0, i = !1, r) {
    let { left: s, right: o } = this, l = t + s.length + this.break, a = null;
    return r && r.from <= t + s.length && r.more ? a = s = s.updateHeight(e, t, i, r) : s.updateHeight(e, t, i), r && r.from <= l + o.length && r.more ? a = o = o.updateHeight(e, l, i, r) : o.updateHeight(e, l, i), a ? this.balanced(s, o) : (this.height = this.left.height + this.right.height, this.outdated = !1, this);
  }
  toString() {
    return this.left + (this.break ? " " : "-") + this.right;
  }
}
function zh(n, e) {
  let t, i;
  n[e] == null && (t = n[e - 1]) instanceof je && (i = n[e + 1]) instanceof je && n.splice(e - 1, 3, new je(t.length + 1 + i.length));
}
const v0 = 5;
class Jl {
  constructor(e, t) {
    this.pos = e, this.oracle = t, this.nodes = [], this.lineStart = -1, this.lineEnd = -1, this.covering = null, this.writtenTo = e;
  }
  get isCovered() {
    return this.covering && this.nodes[this.nodes.length - 1] == this.covering;
  }
  span(e, t) {
    if (this.lineStart > -1) {
      let i = Math.min(t, this.lineEnd), r = this.nodes[this.nodes.length - 1];
      r instanceof vt ? r.length += i - this.pos : (i > this.pos || !this.isCovered) && this.nodes.push(new vt(i - this.pos, -1, 0)), this.writtenTo = i, t > i && (this.nodes.push(null), this.writtenTo++, this.lineStart = -1);
    }
    this.pos = t;
  }
  point(e, t, i) {
    if (e < t || i.heightRelevant) {
      let r = i.widget ? i.widget.estimatedHeight : 0, s = i.widget ? i.widget.lineBreaks : 0;
      r < 0 && (r = this.oracle.lineHeight);
      let o = t - e;
      i.block ? this.addBlock(new Ru(o, r, i)) : (o || s || r >= v0) && this.addLineDeco(r, s, o);
    } else t > e && this.span(e, t);
    this.lineEnd > -1 && this.lineEnd < this.pos && (this.lineEnd = this.oracle.doc.lineAt(this.pos).to);
  }
  enterLine() {
    if (this.lineStart > -1)
      return;
    let { from: e, to: t } = this.oracle.doc.lineAt(this.pos);
    this.lineStart = e, this.lineEnd = t, this.writtenTo < e && ((this.writtenTo < e - 1 || this.nodes[this.nodes.length - 1] == null) && this.nodes.push(this.blankContent(this.writtenTo, e - 1)), this.nodes.push(null)), this.pos > e && this.nodes.push(new vt(this.pos - e, -1, 0)), this.writtenTo = this.pos;
  }
  blankContent(e, t) {
    let i = new je(t - e);
    return this.oracle.doc.lineAt(e).to == t && (i.flags |= 4), i;
  }
  ensureLine() {
    this.enterLine();
    let e = this.nodes.length ? this.nodes[this.nodes.length - 1] : null;
    if (e instanceof vt)
      return e;
    let t = new vt(0, -1, 0);
    return this.nodes.push(t), t;
  }
  addBlock(e) {
    this.enterLine();
    let t = e.deco;
    t && t.startSide > 0 && !this.isCovered && this.ensureLine(), this.nodes.push(e), this.writtenTo = this.pos = this.pos + e.length, t && t.endSide > 0 && (this.covering = e);
  }
  addLineDeco(e, t, i) {
    let r = this.ensureLine();
    r.length += i, r.collapsed += i, r.widgetHeight = Math.max(r.widgetHeight, e), r.breaks += t, this.writtenTo = this.pos = this.pos + i;
  }
  finish(e) {
    let t = this.nodes.length == 0 ? null : this.nodes[this.nodes.length - 1];
    this.lineStart > -1 && !(t instanceof vt) && !this.isCovered ? this.nodes.push(new vt(0, -1, 0)) : (this.writtenTo < this.pos || t == null) && this.nodes.push(this.blankContent(this.writtenTo, this.pos));
    let i = e;
    for (let r of this.nodes)
      r instanceof vt && r.updateHeight(this.oracle, i), i += r ? r.length : 1;
    return this.nodes;
  }
  // Always called with a region that on both sides either stretches
  // to a line break or the end of the document.
  // The returned array uses null to indicate line breaks, but never
  // starts or ends in a line break, or has multiple line breaks next
  // to each other.
  static build(e, t, i, r) {
    let s = new Jl(i, e);
    return ce.spans(t, i, r, s, 0), s.finish(i);
  }
}
function y0(n, e, t) {
  let i = new b0();
  return ce.compare(n, e, t, i, 0), i.changes;
}
class b0 {
  constructor() {
    this.changes = [];
  }
  compareRange() {
  }
  comparePoint(e, t, i, r) {
    (e < t || i && i.heightRelevant || r && r.heightRelevant) && cn(e, t, this.changes, 5);
  }
}
function x0(n, e) {
  let t = n.getBoundingClientRect(), i = n.ownerDocument, r = i.defaultView || window, s = Math.max(0, t.left), o = Math.min(r.innerWidth, t.right), l = Math.max(0, t.top), a = Math.min(r.innerHeight, t.bottom);
  for (let f = n.parentNode; f && f != i.body; )
    if (f.nodeType == 1) {
      let u = f, g = window.getComputedStyle(u);
      if ((u.scrollHeight > u.clientHeight || u.scrollWidth > u.clientWidth) && g.overflow != "visible") {
        let y = u.getBoundingClientRect();
        s = Math.max(s, y.left), o = Math.min(o, y.right), l = Math.max(l, y.top), a = Math.min(f == n.parentNode ? r.innerHeight : a, y.bottom);
      }
      f = g.position == "absolute" || g.position == "fixed" ? u.offsetParent : u.parentNode;
    } else if (f.nodeType == 11)
      f = f.host;
    else
      break;
  return {
    left: s - t.left,
    right: Math.max(s, o) - t.left,
    top: l - (t.top + e),
    bottom: Math.max(l, a) - (t.top + e)
  };
}
function k0(n) {
  let e = n.getBoundingClientRect(), t = n.ownerDocument.defaultView || window;
  return e.left < t.innerWidth && e.right > 0 && e.top < t.innerHeight && e.bottom > 0;
}
function w0(n, e) {
  let t = n.getBoundingClientRect();
  return {
    left: 0,
    right: t.right - t.left,
    top: e,
    bottom: t.bottom - (t.top + e)
  };
}
class mo {
  constructor(e, t, i, r) {
    this.from = e, this.to = t, this.size = i, this.displaySize = r;
  }
  static same(e, t) {
    if (e.length != t.length)
      return !1;
    for (let i = 0; i < e.length; i++) {
      let r = e[i], s = t[i];
      if (r.from != s.from || r.to != s.to || r.size != s.size)
        return !1;
    }
    return !0;
  }
  draw(e, t) {
    return G.replace({
      widget: new S0(this.displaySize * (t ? e.scaleY : e.scaleX), t)
    }).range(this.from, this.to);
  }
}
class S0 extends ci {
  constructor(e, t) {
    super(), this.size = e, this.vertical = t;
  }
  eq(e) {
    return e.size == this.size && e.vertical == this.vertical;
  }
  toDOM() {
    let e = document.createElement("div");
    return this.vertical ? e.style.height = this.size + "px" : (e.style.width = this.size + "px", e.style.height = "2px", e.style.display = "inline-block"), e;
  }
  get estimatedHeight() {
    return this.vertical ? this.size : -1;
  }
}
class $h {
  constructor(e) {
    this.state = e, this.pixelViewport = { left: 0, right: window.innerWidth, top: 0, bottom: 0 }, this.inView = !0, this.paddingTop = 0, this.paddingBottom = 0, this.contentDOMWidth = 0, this.contentDOMHeight = 0, this.editorHeight = 0, this.editorWidth = 0, this.scrollTop = 0, this.scrolledToBottom = !1, this.scaleX = 1, this.scaleY = 1, this.scrollAnchorPos = 0, this.scrollAnchorHeight = -1, this.scaler = qh, this.scrollTarget = null, this.printing = !1, this.mustMeasureContent = !0, this.defaultTextDirection = xe.LTR, this.visibleRanges = [], this.mustEnforceCursorAssoc = !1;
    let t = e.facet(Xl).some((i) => typeof i != "function" && i.class == "cm-lineWrapping");
    this.heightOracle = new d0(t), this.stateDeco = Kh(e), this.heightMap = st.empty().applyChanges(this.stateDeco, ge.empty, this.heightOracle.setDoc(e.doc), [new Ot(0, 0, 0, e.doc.length)]);
    for (let i = 0; i < 2 && (this.viewport = this.getViewport(0, null), !!this.updateForViewport()); i++)
      ;
    this.updateViewportLines(), this.lineGaps = this.ensureLineGaps([]), this.lineGapDeco = G.set(this.lineGaps.map((i) => i.draw(this, !1))), this.computeVisibleRanges();
  }
  updateForViewport() {
    let e = [this.viewport], { main: t } = this.state.selection;
    for (let i = 0; i <= 1; i++) {
      let r = i ? t.head : t.anchor;
      if (!e.some(({ from: s, to: o }) => r >= s && r <= o)) {
        let { from: s, to: o } = this.lineBlockAt(r);
        e.push(new Mr(s, o));
      }
    }
    return this.viewports = e.sort((i, r) => i.from - r.from), this.updateScaler();
  }
  updateScaler() {
    let e = this.scaler;
    return this.scaler = this.heightMap.height <= 7e6 ? qh : new ea(this.heightOracle, this.heightMap, this.viewports), e.eq(this.scaler) ? 0 : 2;
  }
  updateViewportLines() {
    this.viewportLines = [], this.heightMap.forEachLine(this.viewport.from, this.viewport.to, this.heightOracle.setDoc(this.state.doc), 0, 0, (e) => {
      this.viewportLines.push(Vn(e, this.scaler));
    });
  }
  update(e, t = null) {
    this.state = e.state;
    let i = this.stateDeco;
    this.stateDeco = Kh(this.state);
    let r = e.changedRanges, s = Ot.extendWithRanges(r, y0(i, this.stateDeco, e ? e.changes : Fe.empty(this.state.doc.length))), o = this.heightMap.height, l = this.scrolledToBottom ? null : this.scrollAnchorAt(this.scrollTop);
    Hh(), this.heightMap = this.heightMap.applyChanges(this.stateDeco, e.startState.doc, this.heightOracle.setDoc(this.state.doc), s), (this.heightMap.height != o || kn) && (e.flags |= 2), l ? (this.scrollAnchorPos = e.changes.mapPos(l.from, -1), this.scrollAnchorHeight = l.top) : (this.scrollAnchorPos = -1, this.scrollAnchorHeight = o);
    let a = s.length ? this.mapViewport(this.viewport, e.changes) : this.viewport;
    (t && (t.range.head < a.from || t.range.head > a.to) || !this.viewportIsAppropriate(a)) && (a = this.getViewport(0, t));
    let f = a.from != this.viewport.from || a.to != this.viewport.to;
    this.viewport = a, e.flags |= this.updateForViewport(), (f || !e.changes.empty || e.flags & 2) && this.updateViewportLines(), (this.lineGaps.length || this.viewport.to - this.viewport.from > 4e3) && this.updateLineGaps(this.ensureLineGaps(this.mapLineGaps(this.lineGaps, e.changes))), e.flags |= this.computeVisibleRanges(e.changes), t && (this.scrollTarget = t), !this.mustEnforceCursorAssoc && (e.selectionSet || e.focusChanged) && e.view.lineWrapping && e.state.selection.main.empty && e.state.selection.main.assoc && !e.state.facet(au) && (this.mustEnforceCursorAssoc = !0);
  }
  measure(e) {
    let t = e.contentDOM, i = window.getComputedStyle(t), r = this.heightOracle, s = i.whiteSpace;
    this.defaultTextDirection = i.direction == "rtl" ? xe.RTL : xe.LTR;
    let o = this.heightOracle.mustRefreshForWrapping(s) || this.mustMeasureContent, l = t.getBoundingClientRect(), a = o || this.mustMeasureContent || this.contentDOMHeight != l.height;
    this.contentDOMHeight = l.height, this.mustMeasureContent = !1;
    let f = 0, u = 0;
    if (l.width && l.height) {
      let { scaleX: z, scaleY: F } = Kf(t, l);
      (z > 5e-3 && Math.abs(this.scaleX - z) > 5e-3 || F > 5e-3 && Math.abs(this.scaleY - F) > 5e-3) && (this.scaleX = z, this.scaleY = F, f |= 16, o = a = !0);
    }
    let g = (parseInt(i.paddingTop) || 0) * this.scaleY, y = (parseInt(i.paddingBottom) || 0) * this.scaleY;
    (this.paddingTop != g || this.paddingBottom != y) && (this.paddingTop = g, this.paddingBottom = y, f |= 18), this.editorWidth != e.scrollDOM.clientWidth && (r.lineWrapping && (a = !0), this.editorWidth = e.scrollDOM.clientWidth, f |= 16);
    let b = e.scrollDOM.scrollTop * this.scaleY;
    this.scrollTop != b && (this.scrollAnchorHeight = -1, this.scrollTop = b), this.scrolledToBottom = jf(e.scrollDOM);
    let k = (this.printing ? w0 : x0)(t, this.paddingTop), C = k.top - this.pixelViewport.top, M = k.bottom - this.pixelViewport.bottom;
    this.pixelViewport = k;
    let R = this.pixelViewport.bottom > this.pixelViewport.top && this.pixelViewport.right > this.pixelViewport.left;
    if (R != this.inView && (this.inView = R, R && (a = !0)), !this.inView && !this.scrollTarget && !k0(e.dom))
      return 0;
    let I = l.width;
    if ((this.contentDOMWidth != I || this.editorHeight != e.scrollDOM.clientHeight) && (this.contentDOMWidth = l.width, this.editorHeight = e.scrollDOM.clientHeight, f |= 16), a) {
      let z = e.docView.measureVisibleLineHeights(this.viewport);
      if (r.mustRefreshForHeights(z) && (o = !0), o || r.lineWrapping && Math.abs(I - this.contentDOMWidth) > r.charWidth) {
        let { lineHeight: F, charWidth: H, textHeight: Q } = e.docView.measureTextSize();
        o = F > 0 && r.refresh(s, F, H, Q, Math.max(5, I / H), z), o && (e.docView.minWidth = 0, f |= 16);
      }
      C > 0 && M > 0 ? u = Math.max(C, M) : C < 0 && M < 0 && (u = Math.min(C, M)), Hh();
      for (let F of this.viewports) {
        let H = F.from == this.viewport.from ? z : e.docView.measureVisibleLineHeights(F);
        this.heightMap = (o ? st.empty().applyChanges(this.stateDeco, ge.empty, this.heightOracle, [new Ot(0, 0, 0, e.state.doc.length)]) : this.heightMap).updateHeight(r, 0, o, new p0(F.from, H));
      }
      kn && (f |= 2);
    }
    let N = !this.viewportIsAppropriate(this.viewport, u) || this.scrollTarget && (this.scrollTarget.range.head < this.viewport.from || this.scrollTarget.range.head > this.viewport.to);
    return N && (f & 2 && (f |= this.updateScaler()), this.viewport = this.getViewport(u, this.scrollTarget), f |= this.updateForViewport()), (f & 2 || N) && this.updateViewportLines(), (this.lineGaps.length || this.viewport.to - this.viewport.from > 4e3) && this.updateLineGaps(this.ensureLineGaps(o ? [] : this.lineGaps, e)), f |= this.computeVisibleRanges(), this.mustEnforceCursorAssoc && (this.mustEnforceCursorAssoc = !1, e.docView.enforceCursorAssoc()), f;
  }
  get visibleTop() {
    return this.scaler.fromDOM(this.pixelViewport.top);
  }
  get visibleBottom() {
    return this.scaler.fromDOM(this.pixelViewport.bottom);
  }
  getViewport(e, t) {
    let i = 0.5 - Math.max(-0.5, Math.min(0.5, e / 1e3 / 2)), r = this.heightMap, s = this.heightOracle, { visibleTop: o, visibleBottom: l } = this, a = new Mr(r.lineAt(o - i * 1e3, Oe.ByHeight, s, 0, 0).from, r.lineAt(l + (1 - i) * 1e3, Oe.ByHeight, s, 0, 0).to);
    if (t) {
      let { head: f } = t.range;
      if (f < a.from || f > a.to) {
        let u = Math.min(this.editorHeight, this.pixelViewport.bottom - this.pixelViewport.top), g = r.lineAt(f, Oe.ByPos, s, 0, 0), y;
        t.y == "center" ? y = (g.top + g.bottom) / 2 - u / 2 : t.y == "start" || t.y == "nearest" && f < a.from ? y = g.top : y = g.bottom - u, a = new Mr(r.lineAt(y - 1e3 / 2, Oe.ByHeight, s, 0, 0).from, r.lineAt(y + u + 1e3 / 2, Oe.ByHeight, s, 0, 0).to);
      }
    }
    return a;
  }
  mapViewport(e, t) {
    let i = t.mapPos(e.from, -1), r = t.mapPos(e.to, 1);
    return new Mr(this.heightMap.lineAt(i, Oe.ByPos, this.heightOracle, 0, 0).from, this.heightMap.lineAt(r, Oe.ByPos, this.heightOracle, 0, 0).to);
  }
  // Checks if a given viewport covers the visible part of the
  // document and not too much beyond that.
  viewportIsAppropriate({ from: e, to: t }, i = 0) {
    if (!this.inView)
      return !0;
    let { top: r } = this.heightMap.lineAt(e, Oe.ByPos, this.heightOracle, 0, 0), { bottom: s } = this.heightMap.lineAt(t, Oe.ByPos, this.heightOracle, 0, 0), { visibleTop: o, visibleBottom: l } = this;
    return (e == 0 || r <= o - Math.max(10, Math.min(
      -i,
      250
      /* VP.MaxCoverMargin */
    ))) && (t == this.state.doc.length || s >= l + Math.max(10, Math.min(
      i,
      250
      /* VP.MaxCoverMargin */
    ))) && r > o - 2 * 1e3 && s < l + 2 * 1e3;
  }
  mapLineGaps(e, t) {
    if (!e.length || t.empty)
      return e;
    let i = [];
    for (let r of e)
      t.touchesRange(r.from, r.to) || i.push(new mo(t.mapPos(r.from), t.mapPos(r.to), r.size, r.displaySize));
    return i;
  }
  // Computes positions in the viewport where the start or end of a
  // line should be hidden, trying to reuse existing line gaps when
  // appropriate to avoid unneccesary redraws.
  // Uses crude character-counting for the positioning and sizing,
  // since actual DOM coordinates aren't always available and
  // predictable. Relies on generous margins (see LG.Margin) to hide
  // the artifacts this might produce from the user.
  ensureLineGaps(e, t) {
    let i = this.heightOracle.lineWrapping, r = i ? 1e4 : 2e3, s = r >> 1, o = r << 1;
    if (this.defaultTextDirection != xe.LTR && !i)
      return [];
    let l = [], a = (u, g, y, b) => {
      if (g - u < s)
        return;
      let k = this.state.selection.main, C = [k.from];
      k.empty || C.push(k.to);
      for (let R of C)
        if (R > u && R < g) {
          a(u, R - 10, y, b), a(R + 10, g, y, b);
          return;
        }
      let M = O0(e, (R) => R.from >= y.from && R.to <= y.to && Math.abs(R.from - u) < s && Math.abs(R.to - g) < s && !C.some((I) => R.from < I && R.to > I));
      if (!M) {
        if (g < y.to && t && i && t.visibleRanges.some((N) => N.from <= g && N.to >= g)) {
          let N = t.moveToLineBoundary(E.cursor(g), !1, !0).head;
          N > u && (g = N);
        }
        let R = this.gapSize(y, u, g, b), I = i || R < 2e6 ? R : 2e6;
        M = new mo(u, g, R, I);
      }
      l.push(M);
    }, f = (u) => {
      if (u.length < o || u.type != Ye.Text)
        return;
      let g = C0(u.from, u.to, this.stateDeco);
      if (g.total < o)
        return;
      let y = this.scrollTarget ? this.scrollTarget.range.head : null, b, k;
      if (i) {
        let C = r / this.heightOracle.lineLength * this.heightOracle.lineHeight, M, R;
        if (y != null) {
          let I = Pr(g, y), N = ((this.visibleBottom - this.visibleTop) / 2 + C) / u.height;
          M = I - N, R = I + N;
        } else
          M = (this.visibleTop - u.top - C) / u.height, R = (this.visibleBottom - u.top + C) / u.height;
        b = Tr(g, M), k = Tr(g, R);
      } else {
        let C = g.total * this.heightOracle.charWidth, M = r * this.heightOracle.charWidth, R = 0;
        if (C > 2e6)
          for (let H of e)
            H.from >= u.from && H.from < u.to && H.size != H.displaySize && H.from * this.heightOracle.charWidth + R < this.pixelViewport.left && (R = H.size - H.displaySize);
        let I = this.pixelViewport.left + R, N = this.pixelViewport.right + R, z, F;
        if (y != null) {
          let H = Pr(g, y), Q = ((N - I) / 2 + M) / C;
          z = H - Q, F = H + Q;
        } else
          z = (I - M) / C, F = (N + M) / C;
        b = Tr(g, z), k = Tr(g, F);
      }
      b > u.from && a(u.from, b, u, g), k < u.to && a(k, u.to, u, g);
    };
    for (let u of this.viewportLines)
      Array.isArray(u.type) ? u.type.forEach(f) : f(u);
    return l;
  }
  gapSize(e, t, i, r) {
    let s = Pr(r, i) - Pr(r, t);
    return this.heightOracle.lineWrapping ? e.height * s : r.total * this.heightOracle.charWidth * s;
  }
  updateLineGaps(e) {
    mo.same(e, this.lineGaps) || (this.lineGaps = e, this.lineGapDeco = G.set(e.map((t) => t.draw(this, this.heightOracle.lineWrapping))));
  }
  computeVisibleRanges(e) {
    let t = this.stateDeco;
    this.lineGaps.length && (t = t.concat(this.lineGapDeco));
    let i = [];
    ce.spans(t, this.viewport.from, this.viewport.to, {
      span(s, o) {
        i.push({ from: s, to: o });
      },
      point() {
      }
    }, 20);
    let r = 0;
    if (i.length != this.visibleRanges.length)
      r = 12;
    else
      for (let s = 0; s < i.length && !(r & 8); s++) {
        let o = this.visibleRanges[s], l = i[s];
        (o.from != l.from || o.to != l.to) && (r |= 4, e && e.mapPos(o.from, -1) == l.from && e.mapPos(o.to, 1) == l.to || (r |= 8));
      }
    return this.visibleRanges = i, r;
  }
  lineBlockAt(e) {
    return e >= this.viewport.from && e <= this.viewport.to && this.viewportLines.find((t) => t.from <= e && t.to >= e) || Vn(this.heightMap.lineAt(e, Oe.ByPos, this.heightOracle, 0, 0), this.scaler);
  }
  lineBlockAtHeight(e) {
    return e >= this.viewportLines[0].top && e <= this.viewportLines[this.viewportLines.length - 1].bottom && this.viewportLines.find((t) => t.top <= e && t.bottom >= e) || Vn(this.heightMap.lineAt(this.scaler.fromDOM(e), Oe.ByHeight, this.heightOracle, 0, 0), this.scaler);
  }
  scrollAnchorAt(e) {
    let t = this.lineBlockAtHeight(e + 8);
    return t.from >= this.viewport.from || this.viewportLines[0].top - e > 200 ? t : this.viewportLines[0];
  }
  elementAtHeight(e) {
    return Vn(this.heightMap.blockAt(this.scaler.fromDOM(e), this.heightOracle, 0, 0), this.scaler);
  }
  get docHeight() {
    return this.scaler.toDOM(this.heightMap.height);
  }
  get contentHeight() {
    return this.docHeight + this.paddingTop + this.paddingBottom;
  }
}
class Mr {
  constructor(e, t) {
    this.from = e, this.to = t;
  }
}
function C0(n, e, t) {
  let i = [], r = n, s = 0;
  return ce.spans(t, n, e, {
    span() {
    },
    point(o, l) {
      o > r && (i.push({ from: r, to: o }), s += o - r), r = l;
    }
  }, 20), r < e && (i.push({ from: r, to: e }), s += e - r), { total: s, ranges: i };
}
function Tr({ total: n, ranges: e }, t) {
  if (t <= 0)
    return e[0].from;
  if (t >= 1)
    return e[e.length - 1].to;
  let i = Math.floor(n * t);
  for (let r = 0; ; r++) {
    let { from: s, to: o } = e[r], l = o - s;
    if (i <= l)
      return s + i;
    i -= l;
  }
}
function Pr(n, e) {
  let t = 0;
  for (let { from: i, to: r } of n.ranges) {
    if (e <= r) {
      t += e - i;
      break;
    }
    t += r - i;
  }
  return t / n.total;
}
function O0(n, e) {
  for (let t of n)
    if (e(t))
      return t;
}
const qh = {
  toDOM(n) {
    return n;
  },
  fromDOM(n) {
    return n;
  },
  scale: 1,
  eq(n) {
    return n == this;
  }
};
function Kh(n) {
  let e = n.facet(Es).filter((i) => typeof i != "function"), t = n.facet(Yl).filter((i) => typeof i != "function");
  return t.length && e.push(ce.join(t)), e;
}
class ea {
  constructor(e, t, i) {
    let r = 0, s = 0, o = 0;
    this.viewports = i.map(({ from: l, to: a }) => {
      let f = t.lineAt(l, Oe.ByPos, e, 0, 0).top, u = t.lineAt(a, Oe.ByPos, e, 0, 0).bottom;
      return r += u - f, { from: l, to: a, top: f, bottom: u, domTop: 0, domBottom: 0 };
    }), this.scale = (7e6 - r) / (t.height - r);
    for (let l of this.viewports)
      l.domTop = o + (l.top - s) * this.scale, o = l.domBottom = l.domTop + (l.bottom - l.top), s = l.bottom;
  }
  toDOM(e) {
    for (let t = 0, i = 0, r = 0; ; t++) {
      let s = t < this.viewports.length ? this.viewports[t] : null;
      if (!s || e < s.top)
        return r + (e - i) * this.scale;
      if (e <= s.bottom)
        return s.domTop + (e - s.top);
      i = s.bottom, r = s.domBottom;
    }
  }
  fromDOM(e) {
    for (let t = 0, i = 0, r = 0; ; t++) {
      let s = t < this.viewports.length ? this.viewports[t] : null;
      if (!s || e < s.domTop)
        return i + (e - r) / this.scale;
      if (e <= s.domBottom)
        return s.top + (e - s.domTop);
      i = s.bottom, r = s.domBottom;
    }
  }
  eq(e) {
    return e instanceof ea ? this.scale == e.scale && this.viewports.length == e.viewports.length && this.viewports.every((t, i) => t.from == e.viewports[i].from && t.to == e.viewports[i].to) : !1;
  }
}
function Vn(n, e) {
  if (e.scale == 1)
    return n;
  let t = e.toDOM(n.top), i = e.toDOM(n.bottom);
  return new It(n.from, n.length, t, i - t, Array.isArray(n._content) ? n._content.map((r) => Vn(r, e)) : n._content);
}
const Rr = /* @__PURE__ */ U.define({ combine: (n) => n.join(" ") }), dl = /* @__PURE__ */ U.define({ combine: (n) => n.indexOf(!0) > -1 }), pl = /* @__PURE__ */ wi.newName(), Lu = /* @__PURE__ */ wi.newName(), Du = /* @__PURE__ */ wi.newName(), Bu = { "&light": "." + Lu, "&dark": "." + Du };
function gl(n, e, t) {
  return new wi(e, {
    finish(i) {
      return /&/.test(i) ? i.replace(/&\w*/, (r) => {
        if (r == "&")
          return n;
        if (!t || !t[r])
          throw new RangeError(`Unsupported selector: ${r}`);
        return t[r];
      }) : n + " " + i;
    }
  });
}
const A0 = /* @__PURE__ */ gl("." + pl, {
  "&": {
    position: "relative !important",
    boxSizing: "border-box",
    "&.cm-focused": {
      // Provide a simple default outline to make sure a focused
      // editor is visually distinct. Can't leave the default behavior
      // because that will apply to the content element, which is
      // inside the scrollable container and doesn't include the
      // gutters. We also can't use an 'auto' outline, since those
      // are, for some reason, drawn behind the element content, which
      // will cause things like the active line background to cover
      // the outline (#297).
      outline: "1px dotted #212121"
    },
    display: "flex !important",
    flexDirection: "column"
  },
  ".cm-scroller": {
    display: "flex !important",
    alignItems: "flex-start !important",
    fontFamily: "monospace",
    lineHeight: 1.4,
    height: "100%",
    overflowX: "auto",
    position: "relative",
    zIndex: 0,
    overflowAnchor: "none"
  },
  ".cm-content": {
    margin: 0,
    flexGrow: 2,
    flexShrink: 0,
    display: "block",
    whiteSpace: "pre",
    wordWrap: "normal",
    // https://github.com/codemirror/dev/issues/456
    boxSizing: "border-box",
    minHeight: "100%",
    padding: "4px 0",
    outline: "none",
    "&[contenteditable=true]": {
      WebkitUserModify: "read-write-plaintext-only"
    }
  },
  ".cm-lineWrapping": {
    whiteSpace_fallback: "pre-wrap",
    // For IE
    whiteSpace: "break-spaces",
    wordBreak: "break-word",
    // For Safari, which doesn't support overflow-wrap: anywhere
    overflowWrap: "anywhere",
    flexShrink: 1
  },
  "&light .cm-content": { caretColor: "black" },
  "&dark .cm-content": { caretColor: "white" },
  ".cm-line": {
    display: "block",
    padding: "0 2px 0 6px"
  },
  ".cm-layer": {
    position: "absolute",
    left: 0,
    top: 0,
    contain: "size style",
    "& > *": {
      position: "absolute"
    }
  },
  "&light .cm-selectionBackground": {
    background: "#d9d9d9"
  },
  "&dark .cm-selectionBackground": {
    background: "#222"
  },
  "&light.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
    background: "#d7d4f0"
  },
  "&dark.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
    background: "#233"
  },
  ".cm-cursorLayer": {
    pointerEvents: "none"
  },
  "&.cm-focused > .cm-scroller > .cm-cursorLayer": {
    animation: "steps(1) cm-blink 1.2s infinite"
  },
  // Two animations defined so that we can switch between them to
  // restart the animation without forcing another style
  // recomputation.
  "@keyframes cm-blink": { "0%": {}, "50%": { opacity: 0 }, "100%": {} },
  "@keyframes cm-blink2": { "0%": {}, "50%": { opacity: 0 }, "100%": {} },
  ".cm-cursor, .cm-dropCursor": {
    borderLeft: "1.2px solid black",
    marginLeft: "-0.6px",
    pointerEvents: "none"
  },
  ".cm-cursor": {
    display: "none"
  },
  "&dark .cm-cursor": {
    borderLeftColor: "#ddd"
  },
  ".cm-dropCursor": {
    position: "absolute"
  },
  "&.cm-focused > .cm-scroller > .cm-cursorLayer .cm-cursor": {
    display: "block"
  },
  ".cm-iso": {
    unicodeBidi: "isolate"
  },
  ".cm-announced": {
    position: "fixed",
    top: "-10000px"
  },
  "@media print": {
    ".cm-announced": { display: "none" }
  },
  "&light .cm-activeLine": { backgroundColor: "#cceeff44" },
  "&dark .cm-activeLine": { backgroundColor: "#99eeff33" },
  "&light .cm-specialChar": { color: "red" },
  "&dark .cm-specialChar": { color: "#f78" },
  ".cm-gutters": {
    flexShrink: 0,
    display: "flex",
    height: "100%",
    boxSizing: "border-box",
    zIndex: 200
  },
  ".cm-gutters-before": { insetInlineStart: 0 },
  ".cm-gutters-after": { insetInlineEnd: 0 },
  "&light .cm-gutters": {
    backgroundColor: "#f5f5f5",
    color: "#6c6c6c",
    border: "0px solid #ddd",
    "&.cm-gutters-before": { borderRightWidth: "1px" },
    "&.cm-gutters-after": { borderLeftWidth: "1px" }
  },
  "&dark .cm-gutters": {
    backgroundColor: "#333338",
    color: "#ccc"
  },
  ".cm-gutter": {
    display: "flex !important",
    // Necessary -- prevents margin collapsing
    flexDirection: "column",
    flexShrink: 0,
    boxSizing: "border-box",
    minHeight: "100%",
    overflow: "hidden"
  },
  ".cm-gutterElement": {
    boxSizing: "border-box"
  },
  ".cm-lineNumbers .cm-gutterElement": {
    padding: "0 3px 0 5px",
    minWidth: "20px",
    textAlign: "right",
    whiteSpace: "nowrap"
  },
  "&light .cm-activeLineGutter": {
    backgroundColor: "#e2f2ff"
  },
  "&dark .cm-activeLineGutter": {
    backgroundColor: "#222227"
  },
  ".cm-panels": {
    boxSizing: "border-box",
    position: "sticky",
    left: 0,
    right: 0,
    zIndex: 300
  },
  "&light .cm-panels": {
    backgroundColor: "#f5f5f5",
    color: "black"
  },
  "&light .cm-panels-top": {
    borderBottom: "1px solid #ddd"
  },
  "&light .cm-panels-bottom": {
    borderTop: "1px solid #ddd"
  },
  "&dark .cm-panels": {
    backgroundColor: "#333338",
    color: "white"
  },
  ".cm-dialog": {
    padding: "2px 19px 4px 6px",
    position: "relative",
    "& label": { fontSize: "80%" }
  },
  ".cm-dialog-close": {
    position: "absolute",
    top: "3px",
    right: "4px",
    backgroundColor: "inherit",
    border: "none",
    font: "inherit",
    fontSize: "14px",
    padding: "0"
  },
  ".cm-tab": {
    display: "inline-block",
    overflow: "hidden",
    verticalAlign: "bottom"
  },
  ".cm-widgetBuffer": {
    verticalAlign: "text-top",
    height: "1em",
    width: 0,
    display: "inline"
  },
  ".cm-placeholder": {
    color: "#888",
    display: "inline-block",
    verticalAlign: "top",
    userSelect: "none"
  },
  ".cm-highlightSpace": {
    backgroundImage: "radial-gradient(circle at 50% 55%, #aaa 20%, transparent 5%)",
    backgroundPosition: "center"
  },
  ".cm-highlightTab": {
    backgroundImage: `url('data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="200" height="20"><path stroke="%23888" stroke-width="1" fill="none" d="M1 10H196L190 5M190 15L196 10M197 4L197 16"/></svg>')`,
    backgroundSize: "auto 100%",
    backgroundPosition: "right 90%",
    backgroundRepeat: "no-repeat"
  },
  ".cm-trailingSpace": {
    backgroundColor: "#ff332255"
  },
  ".cm-button": {
    verticalAlign: "middle",
    color: "inherit",
    fontSize: "70%",
    padding: ".2em 1em",
    borderRadius: "1px"
  },
  "&light .cm-button": {
    backgroundImage: "linear-gradient(#eff1f5, #d9d9df)",
    border: "1px solid #888",
    "&:active": {
      backgroundImage: "linear-gradient(#b4b4b4, #d0d3d6)"
    }
  },
  "&dark .cm-button": {
    backgroundImage: "linear-gradient(#393939, #111)",
    border: "1px solid #888",
    "&:active": {
      backgroundImage: "linear-gradient(#111, #333)"
    }
  },
  ".cm-textfield": {
    verticalAlign: "middle",
    color: "inherit",
    fontSize: "70%",
    border: "1px solid silver",
    padding: ".2em .5em"
  },
  "&light .cm-textfield": {
    backgroundColor: "white"
  },
  "&dark .cm-textfield": {
    border: "1px solid #555",
    backgroundColor: "inherit"
  }
}, Bu), M0 = {
  childList: !0,
  characterData: !0,
  subtree: !0,
  attributes: !0,
  characterDataOldValue: !0
}, vo = j.ie && j.ie_version <= 11;
class T0 {
  constructor(e) {
    this.view = e, this.active = !1, this.editContext = null, this.selectionRange = new sm(), this.selectionChanged = !1, this.delayedFlush = -1, this.resizeTimeout = -1, this.queue = [], this.delayedAndroidKey = null, this.flushingAndroidKey = -1, this.lastChange = 0, this.scrollTargets = [], this.intersection = null, this.resizeScroll = null, this.intersecting = !1, this.gapIntersection = null, this.gaps = [], this.printQuery = null, this.parentCheck = -1, this.dom = e.contentDOM, this.observer = new MutationObserver((t) => {
      for (let i of t)
        this.queue.push(i);
      (j.ie && j.ie_version <= 11 || j.ios && e.composing) && t.some((i) => i.type == "childList" && i.removedNodes.length || i.type == "characterData" && i.oldValue.length > i.target.nodeValue.length) ? this.flushSoon() : this.flush();
    }), window.EditContext && j.android && e.constructor.EDIT_CONTEXT !== !1 && // Chrome <126 doesn't support inverted selections in edit context (#1392)
    !(j.chrome && j.chrome_version < 126) && (this.editContext = new R0(e), e.state.facet(ri) && (e.contentDOM.editContext = this.editContext.editContext)), vo && (this.onCharData = (t) => {
      this.queue.push({
        target: t.target,
        type: "characterData",
        oldValue: t.prevValue
      }), this.flushSoon();
    }), this.onSelectionChange = this.onSelectionChange.bind(this), this.onResize = this.onResize.bind(this), this.onPrint = this.onPrint.bind(this), this.onScroll = this.onScroll.bind(this), window.matchMedia && (this.printQuery = window.matchMedia("print")), typeof ResizeObserver == "function" && (this.resizeScroll = new ResizeObserver(() => {
      var t;
      ((t = this.view.docView) === null || t === void 0 ? void 0 : t.lastUpdate) < Date.now() - 75 && this.onResize();
    }), this.resizeScroll.observe(e.scrollDOM)), this.addWindowListeners(this.win = e.win), this.start(), typeof IntersectionObserver == "function" && (this.intersection = new IntersectionObserver((t) => {
      this.parentCheck < 0 && (this.parentCheck = setTimeout(this.listenForScroll.bind(this), 1e3)), t.length > 0 && t[t.length - 1].intersectionRatio > 0 != this.intersecting && (this.intersecting = !this.intersecting, this.intersecting != this.view.inView && this.onScrollChanged(document.createEvent("Event")));
    }, { threshold: [0, 1e-3] }), this.intersection.observe(this.dom), this.gapIntersection = new IntersectionObserver((t) => {
      t.length > 0 && t[t.length - 1].intersectionRatio > 0 && this.onScrollChanged(document.createEvent("Event"));
    }, {})), this.listenForScroll(), this.readSelectionRange();
  }
  onScrollChanged(e) {
    this.view.inputState.runHandlers("scroll", e), this.intersecting && this.view.measure();
  }
  onScroll(e) {
    this.intersecting && this.flush(!1), this.editContext && this.view.requestMeasure(this.editContext.measureReq), this.onScrollChanged(e);
  }
  onResize() {
    this.resizeTimeout < 0 && (this.resizeTimeout = setTimeout(() => {
      this.resizeTimeout = -1, this.view.requestMeasure();
    }, 50));
  }
  onPrint(e) {
    (e.type == "change" || !e.type) && !e.matches || (this.view.viewState.printing = !0, this.view.measure(), setTimeout(() => {
      this.view.viewState.printing = !1, this.view.requestMeasure();
    }, 500));
  }
  updateGaps(e) {
    if (this.gapIntersection && (e.length != this.gaps.length || this.gaps.some((t, i) => t != e[i]))) {
      this.gapIntersection.disconnect();
      for (let t of e)
        this.gapIntersection.observe(t);
      this.gaps = e;
    }
  }
  onSelectionChange(e) {
    let t = this.selectionChanged;
    if (!this.readSelectionRange() || this.delayedAndroidKey)
      return;
    let { view: i } = this, r = this.selectionRange;
    if (i.state.facet(ri) ? i.root.activeElement != this.dom : !Kn(this.dom, r))
      return;
    let s = r.anchorNode && i.docView.tile.nearest(r.anchorNode);
    if (s && s.isWidget() && s.widget.ignoreEvent(e)) {
      t || (this.selectionChanged = !1);
      return;
    }
    (j.ie && j.ie_version <= 11 || j.android && j.chrome) && !i.state.selection.main.empty && // (Selection.isCollapsed isn't reliable on IE)
    r.focusNode && _n(r.focusNode, r.focusOffset, r.anchorNode, r.anchorOffset) ? this.flushSoon() : this.flush(!1);
  }
  readSelectionRange() {
    let { view: e } = this, t = yn(e.root);
    if (!t)
      return !1;
    let i = j.safari && e.root.nodeType == 11 && e.root.activeElement == this.dom && P0(this.view, t) || t;
    if (!i || this.selectionRange.eq(i))
      return !1;
    let r = Kn(this.dom, i);
    return r && !this.selectionChanged && e.inputState.lastFocusTime > Date.now() - 200 && e.inputState.lastTouchTime < Date.now() - 300 && lm(this.dom, i) ? (this.view.inputState.lastFocusTime = 0, e.docView.updateSelection(), !1) : (this.selectionRange.setRange(i), r && (this.selectionChanged = !0), !0);
  }
  setSelectionRange(e, t) {
    this.selectionRange.set(e.node, e.offset, t.node, t.offset), this.selectionChanged = !1;
  }
  clearSelectionRange() {
    this.selectionRange.set(null, 0, null, 0);
  }
  listenForScroll() {
    this.parentCheck = -1;
    let e = 0, t = null;
    for (let i = this.dom; i; )
      if (i.nodeType == 1)
        !t && e < this.scrollTargets.length && this.scrollTargets[e] == i ? e++ : t || (t = this.scrollTargets.slice(0, e)), t && t.push(i), i = i.assignedSlot || i.parentNode;
      else if (i.nodeType == 11)
        i = i.host;
      else
        break;
    if (e < this.scrollTargets.length && !t && (t = this.scrollTargets.slice(0, e)), t) {
      for (let i of this.scrollTargets)
        i.removeEventListener("scroll", this.onScroll);
      for (let i of this.scrollTargets = t)
        i.addEventListener("scroll", this.onScroll);
    }
  }
  ignore(e) {
    if (!this.active)
      return e();
    try {
      return this.stop(), e();
    } finally {
      this.start(), this.clear();
    }
  }
  start() {
    this.active || (this.observer.observe(this.dom, M0), vo && this.dom.addEventListener("DOMCharacterDataModified", this.onCharData), this.active = !0);
  }
  stop() {
    this.active && (this.active = !1, this.observer.disconnect(), vo && this.dom.removeEventListener("DOMCharacterDataModified", this.onCharData));
  }
  // Throw away any pending changes
  clear() {
    this.processRecords(), this.queue.length = 0, this.selectionChanged = !1;
  }
  // Chrome Android, especially in combination with GBoard, not only
  // doesn't reliably fire regular key events, but also often
  // surrounds the effect of enter or backspace with a bunch of
  // composition events that, when interrupted, cause text duplication
  // or other kinds of corruption. This hack makes the editor back off
  // from handling DOM changes for a moment when such a key is
  // detected (via beforeinput or keydown), and then tries to flush
  // them or, if that has no effect, dispatches the given key.
  delayAndroidKey(e, t) {
    var i;
    if (!this.delayedAndroidKey) {
      let r = () => {
        let s = this.delayedAndroidKey;
        s && (this.clearDelayedAndroidKey(), this.view.inputState.lastKeyCode = s.keyCode, this.view.inputState.lastKeyTime = Date.now(), !this.flush() && s.force && fn(this.dom, s.key, s.keyCode));
      };
      this.flushingAndroidKey = this.view.win.requestAnimationFrame(r);
    }
    (!this.delayedAndroidKey || e == "Enter") && (this.delayedAndroidKey = {
      key: e,
      keyCode: t,
      // Only run the key handler when no changes are detected if
      // this isn't coming right after another change, in which case
      // it is probably part of a weird chain of updates, and should
      // be ignored if it returns the DOM to its previous state.
      force: this.lastChange < Date.now() - 50 || !!(!((i = this.delayedAndroidKey) === null || i === void 0) && i.force)
    });
  }
  clearDelayedAndroidKey() {
    this.win.cancelAnimationFrame(this.flushingAndroidKey), this.delayedAndroidKey = null, this.flushingAndroidKey = -1;
  }
  flushSoon() {
    this.delayedFlush < 0 && (this.delayedFlush = this.view.win.requestAnimationFrame(() => {
      this.delayedFlush = -1, this.flush();
    }));
  }
  forceFlush() {
    this.delayedFlush >= 0 && (this.view.win.cancelAnimationFrame(this.delayedFlush), this.delayedFlush = -1), this.flush();
  }
  pendingRecords() {
    for (let e of this.observer.takeRecords())
      this.queue.push(e);
    return this.queue;
  }
  processRecords() {
    let e = this.pendingRecords();
    e.length && (this.queue = []);
    let t = -1, i = -1, r = !1;
    for (let s of e) {
      let o = this.readMutation(s);
      o && (o.typeOver && (r = !0), t == -1 ? { from: t, to: i } = o : (t = Math.min(o.from, t), i = Math.max(o.to, i)));
    }
    return { from: t, to: i, typeOver: r };
  }
  readChange() {
    let { from: e, to: t, typeOver: i } = this.processRecords(), r = this.selectionChanged && Kn(this.dom, this.selectionRange);
    if (e < 0 && !r)
      return null;
    e > -1 && (this.lastChange = Date.now()), this.view.inputState.lastFocusTime = 0, this.selectionChanged = !1;
    let s = new jm(this.view, e, t, i);
    return this.view.docView.domChanged = { newSel: s.newSel ? s.newSel.main : null }, s;
  }
  // Apply pending changes, if any
  flush(e = !0) {
    if (this.delayedFlush >= 0 || this.delayedAndroidKey)
      return !1;
    e && this.readSelectionRange();
    let t = this.readChange();
    if (!t)
      return this.view.requestMeasure(), !1;
    let i = this.view.state, r = xu(this.view, t);
    return this.view.state == i && (t.domChanged || t.newSel && !hs(this.view.state.selection, t.newSel.main)) && this.view.update([]), r;
  }
  readMutation(e) {
    let t = this.view.docView.tile.nearest(e.target);
    if (!t || t.isWidget())
      return null;
    if (t.markDirty(e.type == "attributes"), e.type == "childList") {
      let i = _h(t, e.previousSibling || e.target.previousSibling, -1), r = _h(t, e.nextSibling || e.target.nextSibling, 1);
      return {
        from: i ? t.posAfter(i) : t.posAtStart,
        to: r ? t.posBefore(r) : t.posAtEnd,
        typeOver: !1
      };
    } else return e.type == "characterData" ? { from: t.posAtStart, to: t.posAtEnd, typeOver: e.target.nodeValue == e.oldValue } : null;
  }
  setWindow(e) {
    e != this.win && (this.removeWindowListeners(this.win), this.win = e, this.addWindowListeners(this.win));
  }
  addWindowListeners(e) {
    e.addEventListener("resize", this.onResize), this.printQuery ? this.printQuery.addEventListener ? this.printQuery.addEventListener("change", this.onPrint) : this.printQuery.addListener(this.onPrint) : e.addEventListener("beforeprint", this.onPrint), e.addEventListener("scroll", this.onScroll), e.document.addEventListener("selectionchange", this.onSelectionChange);
  }
  removeWindowListeners(e) {
    e.removeEventListener("scroll", this.onScroll), e.removeEventListener("resize", this.onResize), this.printQuery ? this.printQuery.removeEventListener ? this.printQuery.removeEventListener("change", this.onPrint) : this.printQuery.removeListener(this.onPrint) : e.removeEventListener("beforeprint", this.onPrint), e.document.removeEventListener("selectionchange", this.onSelectionChange);
  }
  update(e) {
    this.editContext && (this.editContext.update(e), e.startState.facet(ri) != e.state.facet(ri) && (e.view.contentDOM.editContext = e.state.facet(ri) ? this.editContext.editContext : null));
  }
  destroy() {
    var e, t, i;
    this.stop(), (e = this.intersection) === null || e === void 0 || e.disconnect(), (t = this.gapIntersection) === null || t === void 0 || t.disconnect(), (i = this.resizeScroll) === null || i === void 0 || i.disconnect();
    for (let r of this.scrollTargets)
      r.removeEventListener("scroll", this.onScroll);
    this.removeWindowListeners(this.win), clearTimeout(this.parentCheck), clearTimeout(this.resizeTimeout), this.win.cancelAnimationFrame(this.delayedFlush), this.win.cancelAnimationFrame(this.flushingAndroidKey), this.editContext && (this.view.contentDOM.editContext = null, this.editContext.destroy());
  }
}
function _h(n, e, t) {
  for (; e; ) {
    let i = Ne.get(e);
    if (i && i.parent == n)
      return i;
    let r = e.parentNode;
    e = r != n.dom ? r : t > 0 ? e.nextSibling : e.previousSibling;
  }
  return null;
}
function jh(n, e) {
  let t = e.startContainer, i = e.startOffset, r = e.endContainer, s = e.endOffset, o = n.docView.domAtPos(n.state.selection.main.anchor, 1);
  return _n(o.node, o.offset, r, s) && ([t, i, r, s] = [r, s, t, i]), { anchorNode: t, anchorOffset: i, focusNode: r, focusOffset: s };
}
function P0(n, e) {
  if (e.getComposedRanges) {
    let r = e.getComposedRanges(n.root)[0];
    if (r)
      return jh(n, r);
  }
  let t = null;
  function i(r) {
    r.preventDefault(), r.stopImmediatePropagation(), t = r.getTargetRanges()[0];
  }
  return n.contentDOM.addEventListener("beforeinput", i, !0), n.dom.ownerDocument.execCommand("indent"), n.contentDOM.removeEventListener("beforeinput", i, !0), t ? jh(n, t) : null;
}
class R0 {
  constructor(e) {
    this.from = 0, this.to = 0, this.pendingContextChange = null, this.handlers = /* @__PURE__ */ Object.create(null), this.composing = null, this.resetRange(e.state);
    let t = this.editContext = new window.EditContext({
      text: e.state.doc.sliceString(this.from, this.to),
      selectionStart: this.toContextPos(Math.max(this.from, Math.min(this.to, e.state.selection.main.anchor))),
      selectionEnd: this.toContextPos(e.state.selection.main.head)
    });
    this.handlers.textupdate = (i) => {
      let r = e.state.selection.main, { anchor: s, head: o } = r, l = this.toEditorPos(i.updateRangeStart), a = this.toEditorPos(i.updateRangeEnd);
      e.inputState.composing >= 0 && !this.composing && (this.composing = { contextBase: i.updateRangeStart, editorBase: l, drifted: !1 });
      let f = a - l > i.text.length;
      l == this.from && s < this.from ? l = s : a == this.to && s > this.to && (a = s);
      let u = ku(e.state.sliceDoc(l, a), i.text, (f ? r.from : r.to) - l, f ? "end" : null);
      if (!u) {
        let y = E.single(this.toEditorPos(i.selectionStart), this.toEditorPos(i.selectionEnd));
        hs(y, r) || e.dispatch({ selection: y, userEvent: "select" });
        return;
      }
      let g = {
        from: u.from + l,
        to: u.toA + l,
        insert: ge.of(i.text.slice(u.from, u.toB).split(`
`))
      };
      if ((j.mac || j.android) && g.from == o - 1 && /^\. ?$/.test(i.text) && e.contentDOM.getAttribute("autocorrect") == "off" && (g = { from: l, to: a, insert: ge.of([i.text.replace(".", " ")]) }), this.pendingContextChange = g, !e.state.readOnly) {
        let y = this.to - this.from + (g.to - g.from + g.insert.length);
        Zl(e, g, E.single(this.toEditorPos(i.selectionStart, y), this.toEditorPos(i.selectionEnd, y)));
      }
      this.pendingContextChange && (this.revertPending(e.state), this.setSelection(e.state)), g.from < g.to && !g.insert.length && e.inputState.composing >= 0 && !/[\\p{Alphabetic}\\p{Number}_]/.test(t.text.slice(Math.max(0, i.updateRangeStart - 1), Math.min(t.text.length, i.updateRangeStart + 1))) && this.handlers.compositionend(i);
    }, this.handlers.characterboundsupdate = (i) => {
      let r = [], s = null;
      for (let o = this.toEditorPos(i.rangeStart), l = this.toEditorPos(i.rangeEnd); o < l; o++) {
        let a = e.coordsForChar(o);
        s = a && new DOMRect(a.left, a.top, a.right - a.left, a.bottom - a.top) || s || new DOMRect(), r.push(s);
      }
      t.updateCharacterBounds(i.rangeStart, r);
    }, this.handlers.textformatupdate = (i) => {
      let r = [];
      for (let s of i.getTextFormats()) {
        let o = s.underlineStyle, l = s.underlineThickness;
        if (!/none/i.test(o) && !/none/i.test(l)) {
          let a = this.toEditorPos(s.rangeStart), f = this.toEditorPos(s.rangeEnd);
          if (a < f) {
            let u = `text-decoration: underline ${/^[a-z]/.test(o) ? o + " " : o == "Dashed" ? "dashed " : o == "Squiggle" ? "wavy " : ""}${/thin/i.test(l) ? 1 : 2}px`;
            r.push(G.mark({ attributes: { style: u } }).range(a, f));
          }
        }
      }
      e.dispatch({ effects: cu.of(G.set(r)) });
    }, this.handlers.compositionstart = () => {
      e.inputState.composing < 0 && (e.inputState.composing = 0, e.inputState.compositionFirstChange = !0);
    }, this.handlers.compositionend = () => {
      if (e.inputState.composing = -1, e.inputState.compositionFirstChange = null, this.composing) {
        let { drifted: i } = this.composing;
        this.composing = null, i && this.reset(e.state);
      }
    };
    for (let i in this.handlers)
      t.addEventListener(i, this.handlers[i]);
    this.measureReq = { read: (i) => {
      this.editContext.updateControlBounds(i.contentDOM.getBoundingClientRect());
      let r = yn(i.root);
      r && r.rangeCount && this.editContext.updateSelectionBounds(r.getRangeAt(0).getBoundingClientRect());
    } };
  }
  applyEdits(e) {
    let t = 0, i = !1, r = this.pendingContextChange;
    return e.changes.iterChanges((s, o, l, a, f) => {
      if (i)
        return;
      let u = f.length - (o - s);
      if (r && o >= r.to)
        if (r.from == s && r.to == o && r.insert.eq(f)) {
          r = this.pendingContextChange = null, t += u, this.to += u;
          return;
        } else
          r = null, this.revertPending(e.state);
      if (s += t, o += t, o <= this.from)
        this.from += u, this.to += u;
      else if (s < this.to) {
        if (s < this.from || o > this.to || this.to - this.from + f.length > 3e4) {
          i = !0;
          return;
        }
        this.editContext.updateText(this.toContextPos(s), this.toContextPos(o), f.toString()), this.to += u;
      }
      t += u;
    }), r && !i && this.revertPending(e.state), !i;
  }
  update(e) {
    let t = this.pendingContextChange, i = e.startState.selection.main;
    this.composing && (this.composing.drifted || !e.changes.touchesRange(i.from, i.to) && e.transactions.some((r) => !r.isUserEvent("input.type") && r.changes.touchesRange(this.from, this.to))) ? (this.composing.drifted = !0, this.composing.editorBase = e.changes.mapPos(this.composing.editorBase)) : !this.applyEdits(e) || !this.rangeIsValid(e.state) ? (this.pendingContextChange = null, this.reset(e.state)) : (e.docChanged || e.selectionSet || t) && this.setSelection(e.state), (e.geometryChanged || e.docChanged || e.selectionSet) && e.view.requestMeasure(this.measureReq);
  }
  resetRange(e) {
    let { head: t } = e.selection.main;
    this.from = Math.max(
      0,
      t - 1e4
      /* CxVp.Margin */
    ), this.to = Math.min(
      e.doc.length,
      t + 1e4
      /* CxVp.Margin */
    );
  }
  reset(e) {
    this.resetRange(e), this.editContext.updateText(0, this.editContext.text.length, e.doc.sliceString(this.from, this.to)), this.setSelection(e);
  }
  revertPending(e) {
    let t = this.pendingContextChange;
    this.pendingContextChange = null, this.editContext.updateText(this.toContextPos(t.from), this.toContextPos(t.from + t.insert.length), e.doc.sliceString(t.from, t.to));
  }
  setSelection(e) {
    let { main: t } = e.selection, i = this.toContextPos(Math.max(this.from, Math.min(this.to, t.anchor))), r = this.toContextPos(t.head);
    (this.editContext.selectionStart != i || this.editContext.selectionEnd != r) && this.editContext.updateSelection(i, r);
  }
  rangeIsValid(e) {
    let { head: t } = e.selection.main;
    return !(this.from > 0 && t - this.from < 500 || this.to < e.doc.length && this.to - t < 500 || this.to - this.from > 1e4 * 3);
  }
  toEditorPos(e, t = this.to - this.from) {
    e = Math.min(e, t);
    let i = this.composing;
    return i && i.drifted ? i.editorBase + (e - i.contextBase) : e + this.from;
  }
  toContextPos(e) {
    let t = this.composing;
    return t && t.drifted ? t.contextBase + (e - t.editorBase) : e - this.from;
  }
  destroy() {
    for (let e in this.handlers)
      this.editContext.removeEventListener(e, this.handlers[e]);
  }
}
class _ {
  /**
  The current editor state.
  */
  get state() {
    return this.viewState.state;
  }
  /**
  To be able to display large documents without consuming too much
  memory or overloading the browser, CodeMirror only draws the
  code that is visible (plus a margin around it) to the DOM. This
  property tells you the extent of the current drawn viewport, in
  document positions.
  */
  get viewport() {
    return this.viewState.viewport;
  }
  /**
  When there are, for example, large collapsed ranges in the
  viewport, its size can be a lot bigger than the actual visible
  content. Thus, if you are doing something like styling the
  content in the viewport, it is preferable to only do so for
  these ranges, which are the subset of the viewport that is
  actually drawn.
  */
  get visibleRanges() {
    return this.viewState.visibleRanges;
  }
  /**
  Returns false when the editor is entirely scrolled out of view
  or otherwise hidden.
  */
  get inView() {
    return this.viewState.inView;
  }
  /**
  Indicates whether the user is currently composing text via
  [IME](https://en.wikipedia.org/wiki/Input_method), and at least
  one change has been made in the current composition.
  */
  get composing() {
    return !!this.inputState && this.inputState.composing > 0;
  }
  /**
  Indicates whether the user is currently in composing state. Note
  that on some platforms, like Android, this will be the case a
  lot, since just putting the cursor on a word starts a
  composition there.
  */
  get compositionStarted() {
    return !!this.inputState && this.inputState.composing >= 0;
  }
  /**
  The document or shadow root that the view lives in.
  */
  get root() {
    return this._root;
  }
  /**
  @internal
  */
  get win() {
    return this.dom.ownerDocument.defaultView || window;
  }
  /**
  Construct a new view. You'll want to either provide a `parent`
  option, or put `view.dom` into your document after creating a
  view, so that the user can see the editor.
  */
  constructor(e = {}) {
    var t;
    this.plugins = [], this.pluginMap = /* @__PURE__ */ new Map(), this.editorAttrs = {}, this.contentAttrs = {}, this.bidiCache = [], this.destroyed = !1, this.updateState = 2, this.measureScheduled = -1, this.measureRequests = [], this.contentDOM = document.createElement("div"), this.scrollDOM = document.createElement("div"), this.scrollDOM.tabIndex = -1, this.scrollDOM.className = "cm-scroller", this.scrollDOM.appendChild(this.contentDOM), this.announceDOM = document.createElement("div"), this.announceDOM.className = "cm-announced", this.announceDOM.setAttribute("aria-live", "polite"), this.dom = document.createElement("div"), this.dom.appendChild(this.announceDOM), this.dom.appendChild(this.scrollDOM), e.parent && e.parent.appendChild(this.dom);
    let { dispatch: i } = e;
    this.dispatchTransactions = e.dispatchTransactions || i && ((r) => r.forEach((s) => i(s, this))) || ((r) => this.update(r)), this.dispatch = this.dispatch.bind(this), this._root = e.root || om(e.parent) || document, this.viewState = new $h(e.state || pe.create(e)), e.scrollTo && e.scrollTo.is(Cr) && (this.viewState.scrollTarget = e.scrollTo.value.clip(this.viewState.state)), this.plugins = this.state.facet(sn).map((r) => new co(r));
    for (let r of this.plugins)
      r.update(this);
    this.observer = new T0(this), this.inputState = new Gm(this), this.inputState.ensureHandlers(this.plugins), this.docView = new Ph(this), this.mountStyles(), this.updateAttrs(), this.updateState = 0, this.requestMeasure(), !((t = document.fonts) === null || t === void 0) && t.ready && document.fonts.ready.then(() => {
      this.viewState.mustMeasureContent = !0, this.requestMeasure();
    });
  }
  dispatch(...e) {
    let t = e.length == 1 && e[0] instanceof Qe ? e : e.length == 1 && Array.isArray(e[0]) ? e[0] : [this.state.update(...e)];
    this.dispatchTransactions(t, this);
  }
  /**
  Update the view for the given array of transactions. This will
  update the visible document and selection to match the state
  produced by the transactions, and notify view plugins of the
  change. You should usually call
  [`dispatch`](https://codemirror.net/6/docs/ref/#view.EditorView.dispatch) instead, which uses this
  as a primitive.
  */
  update(e) {
    if (this.updateState != 0)
      throw new Error("Calls to EditorView.update are not allowed while an update is in progress");
    let t = !1, i = !1, r, s = this.state;
    for (let y of e) {
      if (y.startState != s)
        throw new RangeError("Trying to update state with a transaction that doesn't start from the previous state.");
      s = y.state;
    }
    if (this.destroyed) {
      this.viewState.state = s;
      return;
    }
    let o = this.hasFocus, l = 0, a = null;
    e.some((y) => y.annotation(Mu)) ? (this.inputState.notifiedFocused = o, l = 1) : o != this.inputState.notifiedFocused && (this.inputState.notifiedFocused = o, a = Tu(s, o), a || (l = 1));
    let f = this.observer.delayedAndroidKey, u = null;
    if (f ? (this.observer.clearDelayedAndroidKey(), u = this.observer.readChange(), (u && !this.state.doc.eq(s.doc) || !this.state.selection.eq(s.selection)) && (u = null)) : this.observer.clear(), s.facet(pe.phrases) != this.state.facet(pe.phrases))
      return this.setState(s);
    r = os.create(this, s, e), r.flags |= l;
    let g = this.viewState.scrollTarget;
    try {
      this.updateState = 2;
      for (let y of e) {
        if (g && (g = g.map(y.changes)), y.scrollIntoView) {
          let { main: b } = y.state.selection;
          g = new un(b.empty ? b : E.cursor(b.head, b.head > b.anchor ? -1 : 1));
        }
        for (let b of y.effects)
          b.is(Cr) && (g = b.value.clip(this.state));
      }
      this.viewState.update(r, g), this.bidiCache = fs.update(this.bidiCache, r.changes), r.empty || (this.updatePlugins(r), this.inputState.update(r)), t = this.docView.update(r), this.state.facet(Wn) != this.styleModules && this.mountStyles(), i = this.updateAttrs(), this.showAnnouncements(e), this.docView.updateSelection(t, e.some((y) => y.isUserEvent("select.pointer")));
    } finally {
      this.updateState = 0;
    }
    if (r.startState.facet(Rr) != r.state.facet(Rr) && (this.viewState.mustMeasureContent = !0), (t || i || g || this.viewState.mustEnforceCursorAssoc || this.viewState.mustMeasureContent) && this.requestMeasure(), t && this.docViewUpdate(), !r.empty)
      for (let y of this.state.facet(al))
        try {
          y(r);
        } catch (b) {
          ft(this.state, b, "update listener");
        }
    (a || u) && Promise.resolve().then(() => {
      a && this.state == a.startState && this.dispatch(a), u && !xu(this, u) && f.force && fn(this.contentDOM, f.key, f.keyCode);
    });
  }
  /**
  Reset the view to the given state. (This will cause the entire
  document to be redrawn and all view plugins to be reinitialized,
  so you should probably only use it when the new state isn't
  derived from the old state. Otherwise, use
  [`dispatch`](https://codemirror.net/6/docs/ref/#view.EditorView.dispatch) instead.)
  */
  setState(e) {
    if (this.updateState != 0)
      throw new Error("Calls to EditorView.setState are not allowed while an update is in progress");
    if (this.destroyed) {
      this.viewState.state = e;
      return;
    }
    this.updateState = 2;
    let t = this.hasFocus;
    try {
      for (let i of this.plugins)
        i.destroy(this);
      this.viewState = new $h(e), this.plugins = e.facet(sn).map((i) => new co(i)), this.pluginMap.clear();
      for (let i of this.plugins)
        i.update(this);
      this.docView.destroy(), this.docView = new Ph(this), this.inputState.ensureHandlers(this.plugins), this.mountStyles(), this.updateAttrs(), this.bidiCache = [];
    } finally {
      this.updateState = 0;
    }
    t && this.focus(), this.requestMeasure();
  }
  updatePlugins(e) {
    let t = e.startState.facet(sn), i = e.state.facet(sn);
    if (t != i) {
      let r = [];
      for (let s of i) {
        let o = t.indexOf(s);
        if (o < 0)
          r.push(new co(s));
        else {
          let l = this.plugins[o];
          l.mustUpdate = e, r.push(l);
        }
      }
      for (let s of this.plugins)
        s.mustUpdate != e && s.destroy(this);
      this.plugins = r, this.pluginMap.clear();
    } else
      for (let r of this.plugins)
        r.mustUpdate = e;
    for (let r = 0; r < this.plugins.length; r++)
      this.plugins[r].update(this);
    t != i && this.inputState.ensureHandlers(this.plugins);
  }
  docViewUpdate() {
    for (let e of this.plugins) {
      let t = e.value;
      if (t && t.docViewUpdate)
        try {
          t.docViewUpdate(this);
        } catch (i) {
          ft(this.state, i, "doc view update listener");
        }
    }
  }
  /**
  @internal
  */
  measure(e = !0) {
    if (this.destroyed)
      return;
    if (this.measureScheduled > -1 && this.win.cancelAnimationFrame(this.measureScheduled), this.observer.delayedAndroidKey) {
      this.measureScheduled = -1, this.requestMeasure();
      return;
    }
    this.measureScheduled = 0, e && this.observer.forceFlush();
    let t = null, i = this.scrollDOM, r = i.scrollTop * this.scaleY, { scrollAnchorPos: s, scrollAnchorHeight: o } = this.viewState;
    Math.abs(r - this.viewState.scrollTop) > 1 && (o = -1), this.viewState.scrollAnchorHeight = -1;
    try {
      for (let l = 0; ; l++) {
        if (o < 0)
          if (jf(i))
            s = -1, o = this.viewState.heightMap.height;
          else {
            let b = this.viewState.scrollAnchorAt(r);
            s = b.from, o = b.top;
          }
        this.updateState = 1;
        let a = this.viewState.measure(this);
        if (!a && !this.measureRequests.length && this.viewState.scrollTarget == null)
          break;
        if (l > 5) {
          console.warn(this.measureRequests.length ? "Measure loop restarted more than 5 times" : "Viewport failed to stabilize");
          break;
        }
        let f = [];
        a & 4 || ([this.measureRequests, f] = [f, this.measureRequests]);
        let u = f.map((b) => {
          try {
            return b.read(this);
          } catch (k) {
            return ft(this.state, k), Uh;
          }
        }), g = os.create(this, this.state, []), y = !1;
        g.flags |= a, t ? t.flags |= a : t = g, this.updateState = 2, g.empty || (this.updatePlugins(g), this.inputState.update(g), this.updateAttrs(), y = this.docView.update(g), y && this.docViewUpdate());
        for (let b = 0; b < f.length; b++)
          if (u[b] != Uh)
            try {
              let k = f[b];
              k.write && k.write(u[b], this);
            } catch (k) {
              ft(this.state, k);
            }
        if (y && this.docView.updateSelection(!0), !g.viewportChanged && this.measureRequests.length == 0) {
          if (this.viewState.editorHeight)
            if (this.viewState.scrollTarget) {
              this.docView.scrollIntoView(this.viewState.scrollTarget), this.viewState.scrollTarget = null, o = -1;
              continue;
            } else {
              let k = (s < 0 ? this.viewState.heightMap.height : this.viewState.lineBlockAt(s).top) - o;
              if (k > 1 || k < -1) {
                r = r + k, i.scrollTop = r / this.scaleY, o = -1;
                continue;
              }
            }
          break;
        }
      }
    } finally {
      this.updateState = 0, this.measureScheduled = -1;
    }
    if (t && !t.empty)
      for (let l of this.state.facet(al))
        l(t);
  }
  /**
  Get the CSS classes for the currently active editor themes.
  */
  get themeClasses() {
    return pl + " " + (this.state.facet(dl) ? Du : Lu) + " " + this.state.facet(Rr);
  }
  updateAttrs() {
    let e = Xh(this, fu, {
      class: "cm-editor" + (this.hasFocus ? " cm-focused " : " ") + this.themeClasses
    }), t = {
      spellcheck: "false",
      autocorrect: "off",
      autocapitalize: "off",
      writingsuggestions: "false",
      translate: "no",
      contenteditable: this.state.facet(ri) ? "true" : "false",
      class: "cm-content",
      style: `${j.tabSize}: ${this.state.tabSize}`,
      role: "textbox",
      "aria-multiline": "true"
    };
    this.state.readOnly && (t["aria-readonly"] = "true"), Xh(this, Xl, t);
    let i = this.observer.ignore(() => {
      let r = Sh(this.contentDOM, this.contentAttrs, t), s = Sh(this.dom, this.editorAttrs, e);
      return r || s;
    });
    return this.editorAttrs = e, this.contentAttrs = t, i;
  }
  showAnnouncements(e) {
    let t = !0;
    for (let i of e)
      for (let r of i.effects)
        if (r.is(_.announce)) {
          t && (this.announceDOM.textContent = ""), t = !1;
          let s = this.announceDOM.appendChild(document.createElement("div"));
          s.textContent = r.value;
        }
  }
  mountStyles() {
    this.styleModules = this.state.facet(Wn);
    let e = this.state.facet(_.cspNonce);
    wi.mount(this.root, this.styleModules.concat(A0).reverse(), e ? { nonce: e } : void 0);
  }
  readMeasured() {
    if (this.updateState == 2)
      throw new Error("Reading the editor layout isn't allowed during an update");
    this.updateState == 0 && this.measureScheduled > -1 && this.measure(!1);
  }
  /**
  Schedule a layout measurement, optionally providing callbacks to
  do custom DOM measuring followed by a DOM write phase. Using
  this is preferable reading DOM layout directly from, for
  example, an event handler, because it'll make sure measuring and
  drawing done by other components is synchronized, avoiding
  unnecessary DOM layout computations.
  */
  requestMeasure(e) {
    if (this.measureScheduled < 0 && (this.measureScheduled = this.win.requestAnimationFrame(() => this.measure())), e) {
      if (this.measureRequests.indexOf(e) > -1)
        return;
      if (e.key != null) {
        for (let t = 0; t < this.measureRequests.length; t++)
          if (this.measureRequests[t].key === e.key) {
            this.measureRequests[t] = e;
            return;
          }
      }
      this.measureRequests.push(e);
    }
  }
  /**
  Get the value of a specific plugin, if present. Note that
  plugins that crash can be dropped from a view, so even when you
  know you registered a given plugin, it is recommended to check
  the return value of this method.
  */
  plugin(e) {
    let t = this.pluginMap.get(e);
    return (t === void 0 || t && t.plugin != e) && this.pluginMap.set(e, t = this.plugins.find((i) => i.plugin == e) || null), t && t.update(this).value;
  }
  /**
  The top position of the document, in screen coordinates. This
  may be negative when the editor is scrolled down. Points
  directly to the top of the first line, not above the padding.
  */
  get documentTop() {
    return this.contentDOM.getBoundingClientRect().top + this.viewState.paddingTop;
  }
  /**
  Reports the padding above and below the document.
  */
  get documentPadding() {
    return { top: this.viewState.paddingTop, bottom: this.viewState.paddingBottom };
  }
  /**
  If the editor is transformed with CSS, this provides the scale
  along the X axis. Otherwise, it will just be 1. Note that
  transforms other than translation and scaling are not supported.
  */
  get scaleX() {
    return this.viewState.scaleX;
  }
  /**
  Provide the CSS transformed scale along the Y axis.
  */
  get scaleY() {
    return this.viewState.scaleY;
  }
  /**
  Find the text line or block widget at the given vertical
  position (which is interpreted as relative to the [top of the
  document](https://codemirror.net/6/docs/ref/#view.EditorView.documentTop)).
  */
  elementAtHeight(e) {
    return this.readMeasured(), this.viewState.elementAtHeight(e);
  }
  /**
  Find the line block (see
  [`lineBlockAt`](https://codemirror.net/6/docs/ref/#view.EditorView.lineBlockAt)) at the given
  height, again interpreted relative to the [top of the
  document](https://codemirror.net/6/docs/ref/#view.EditorView.documentTop).
  */
  lineBlockAtHeight(e) {
    return this.readMeasured(), this.viewState.lineBlockAtHeight(e);
  }
  /**
  Get the extent and vertical position of all [line
  blocks](https://codemirror.net/6/docs/ref/#view.EditorView.lineBlockAt) in the viewport. Positions
  are relative to the [top of the
  document](https://codemirror.net/6/docs/ref/#view.EditorView.documentTop);
  */
  get viewportLineBlocks() {
    return this.viewState.viewportLines;
  }
  /**
  Find the line block around the given document position. A line
  block is a range delimited on both sides by either a
  non-[hidden](https://codemirror.net/6/docs/ref/#view.Decoration^replace) line break, or the
  start/end of the document. It will usually just hold a line of
  text, but may be broken into multiple textblocks by block
  widgets.
  */
  lineBlockAt(e) {
    return this.viewState.lineBlockAt(e);
  }
  /**
  The editor's total content height.
  */
  get contentHeight() {
    return this.viewState.contentHeight;
  }
  /**
  Move a cursor position by [grapheme
  cluster](https://codemirror.net/6/docs/ref/#state.findClusterBreak). `forward` determines whether
  the motion is away from the line start, or towards it. In
  bidirectional text, the line is traversed in visual order, using
  the editor's [text direction](https://codemirror.net/6/docs/ref/#view.EditorView.textDirection).
  When the start position was the last one on the line, the
  returned position will be across the line break. If there is no
  further line, the original position is returned.
  
  By default, this method moves over a single cluster. The
  optional `by` argument can be used to move across more. It will
  be called with the first cluster as argument, and should return
  a predicate that determines, for each subsequent cluster,
  whether it should also be moved over.
  */
  moveByChar(e, t, i) {
    return go(this, e, Rh(this, e, t, i));
  }
  /**
  Move a cursor position across the next group of either
  [letters](https://codemirror.net/6/docs/ref/#state.EditorState.charCategorizer) or non-letter
  non-whitespace characters.
  */
  moveByGroup(e, t) {
    return go(this, e, Rh(this, e, t, (i) => zm(this, e.head, i)));
  }
  /**
  Get the cursor position visually at the start or end of a line.
  Note that this may differ from the _logical_ position at its
  start or end (which is simply at `line.from`/`line.to`) if text
  at the start or end goes against the line's base text direction.
  */
  visualLineSide(e, t) {
    let i = this.bidiSpans(e), r = this.textDirectionAt(e.from), s = i[t ? i.length - 1 : 0];
    return E.cursor(s.side(t, r) + e.from, s.forward(!t, r) ? 1 : -1);
  }
  /**
  Move to the next line boundary in the given direction. If
  `includeWrap` is true, line wrapping is on, and there is a
  further wrap point on the current line, the wrap point will be
  returned. Otherwise this function will return the start or end
  of the line.
  */
  moveToLineBoundary(e, t, i = !0) {
    return Hm(this, e, t, i);
  }
  /**
  Move a cursor position vertically. When `distance` isn't given,
  it defaults to moving to the next line (including wrapped
  lines). Otherwise, `distance` should provide a positive distance
  in pixels.
  
  When `start` has a
  [`goalColumn`](https://codemirror.net/6/docs/ref/#state.SelectionRange.goalColumn), the vertical
  motion will use that as a target horizontal position. Otherwise,
  the cursor's own horizontal position is used. The returned
  cursor will have its goal column set to whichever column was
  used.
  */
  moveVertically(e, t, i) {
    return go(this, e, $m(this, e, t, i));
  }
  /**
  Find the DOM parent node and offset (child offset if `node` is
  an element, character offset when it is a text node) at the
  given document position.
  
  Note that for positions that aren't currently in
  `visibleRanges`, the resulting DOM position isn't necessarily
  meaningful (it may just point before or after a placeholder
  element).
  */
  domAtPos(e, t = 1) {
    return this.docView.domAtPos(e, t);
  }
  /**
  Find the document position at the given DOM node. Can be useful
  for associating positions with DOM events. Will raise an error
  when `node` isn't part of the editor content.
  */
  posAtDOM(e, t = 0) {
    return this.docView.posFromDOM(e, t);
  }
  posAtCoords(e, t = !0) {
    this.readMeasured();
    let i = fl(this, e, t);
    return i && i.pos;
  }
  posAndSideAtCoords(e, t = !0) {
    return this.readMeasured(), fl(this, e, t);
  }
  /**
  Get the screen coordinates at the given document position.
  `side` determines whether the coordinates are based on the
  element before (-1) or after (1) the position (if no element is
  available on the given side, the method will transparently use
  another strategy to get reasonable coordinates).
  */
  coordsAtPos(e, t = 1) {
    this.readMeasured();
    let i = this.docView.coordsAt(e, t);
    if (!i || i.left == i.right)
      return i;
    let r = this.state.doc.lineAt(e), s = this.bidiSpans(r), o = s[si.find(s, e - r.from, -1, t)];
    return ss(i, o.dir == xe.LTR == t > 0);
  }
  /**
  Return the rectangle around a given character. If `pos` does not
  point in front of a character that is in the viewport and
  rendered (i.e. not replaced, not a line break), this will return
  null. For space characters that are a line wrap point, this will
  return the position before the line break.
  */
  coordsForChar(e) {
    return this.readMeasured(), this.docView.coordsForChar(e);
  }
  /**
  The default width of a character in the editor. May not
  accurately reflect the width of all characters (given variable
  width fonts or styling of invididual ranges).
  */
  get defaultCharacterWidth() {
    return this.viewState.heightOracle.charWidth;
  }
  /**
  The default height of a line in the editor. May not be accurate
  for all lines.
  */
  get defaultLineHeight() {
    return this.viewState.heightOracle.lineHeight;
  }
  /**
  The text direction
  ([`direction`](https://developer.mozilla.org/en-US/docs/Web/CSS/direction)
  CSS property) of the editor's content element.
  */
  get textDirection() {
    return this.viewState.defaultTextDirection;
  }
  /**
  Find the text direction of the block at the given position, as
  assigned by CSS. If
  [`perLineTextDirection`](https://codemirror.net/6/docs/ref/#view.EditorView^perLineTextDirection)
  isn't enabled, or the given position is outside of the viewport,
  this will always return the same as
  [`textDirection`](https://codemirror.net/6/docs/ref/#view.EditorView.textDirection). Note that
  this may trigger a DOM layout.
  */
  textDirectionAt(e) {
    return !this.state.facet(lu) || e < this.viewport.from || e > this.viewport.to ? this.textDirection : (this.readMeasured(), this.docView.textDirectionAt(e));
  }
  /**
  Whether this editor [wraps lines](https://codemirror.net/6/docs/ref/#view.EditorView.lineWrapping)
  (as determined by the
  [`white-space`](https://developer.mozilla.org/en-US/docs/Web/CSS/white-space)
  CSS property of its content element).
  */
  get lineWrapping() {
    return this.viewState.heightOracle.lineWrapping;
  }
  /**
  Returns the bidirectional text structure of the given line
  (which should be in the current document) as an array of span
  objects. The order of these spans matches the [text
  direction](https://codemirror.net/6/docs/ref/#view.EditorView.textDirection)—if that is
  left-to-right, the leftmost spans come first, otherwise the
  rightmost spans come first.
  */
  bidiSpans(e) {
    if (e.length > L0)
      return Jf(e.length);
    let t = this.textDirectionAt(e.from), i;
    for (let s of this.bidiCache)
      if (s.from == e.from && s.dir == t && (s.fresh || Zf(s.isolates, i = Ah(this, e))))
        return s.order;
    i || (i = Ah(this, e));
    let r = pm(e.text, t, i);
    return this.bidiCache.push(new fs(e.from, e.to, t, i, !0, r)), r;
  }
  /**
  Check whether the editor has focus.
  */
  get hasFocus() {
    var e;
    return (this.dom.ownerDocument.hasFocus() || j.safari && ((e = this.inputState) === null || e === void 0 ? void 0 : e.lastContextMenu) > Date.now() - 3e4) && this.root.activeElement == this.contentDOM;
  }
  /**
  Put focus on the editor.
  */
  focus() {
    this.observer.ignore(() => {
      _f(this.contentDOM), this.docView.updateSelection();
    });
  }
  /**
  Update the [root](https://codemirror.net/6/docs/ref/##view.EditorViewConfig.root) in which the editor lives. This is only
  necessary when moving the editor's existing DOM to a new window or shadow root.
  */
  setRoot(e) {
    this._root != e && (this._root = e, this.observer.setWindow((e.nodeType == 9 ? e : e.ownerDocument).defaultView || window), this.mountStyles());
  }
  /**
  Clean up this editor view, removing its element from the
  document, unregistering event handlers, and notifying
  plugins. The view instance can no longer be used after
  calling this.
  */
  destroy() {
    this.root.activeElement == this.contentDOM && this.contentDOM.blur();
    for (let e of this.plugins)
      e.destroy(this);
    this.plugins = [], this.inputState.destroy(), this.docView.destroy(), this.dom.remove(), this.observer.destroy(), this.measureScheduled > -1 && this.win.cancelAnimationFrame(this.measureScheduled), this.destroyed = !0;
  }
  /**
  Returns an effect that can be
  [added](https://codemirror.net/6/docs/ref/#state.TransactionSpec.effects) to a transaction to
  cause it to scroll the given position or range into view.
  */
  static scrollIntoView(e, t = {}) {
    return Cr.of(new un(typeof e == "number" ? E.cursor(e) : e, t.y, t.x, t.yMargin, t.xMargin));
  }
  /**
  Return an effect that resets the editor to its current (at the
  time this method was called) scroll position. Note that this
  only affects the editor's own scrollable element, not parents.
  See also
  [`EditorViewConfig.scrollTo`](https://codemirror.net/6/docs/ref/#view.EditorViewConfig.scrollTo).
  
  The effect should be used with a document identical to the one
  it was created for. Failing to do so is not an error, but may
  not scroll to the expected position. You can
  [map](https://codemirror.net/6/docs/ref/#state.StateEffect.map) the effect to account for changes.
  */
  scrollSnapshot() {
    let { scrollTop: e, scrollLeft: t } = this.scrollDOM, i = this.viewState.scrollAnchorAt(e);
    return Cr.of(new un(E.cursor(i.from), "start", "start", i.top - e, t, !0));
  }
  /**
  Enable or disable tab-focus mode, which disables key bindings
  for Tab and Shift-Tab, letting the browser's default
  focus-changing behavior go through instead. This is useful to
  prevent trapping keyboard users in your editor.
  
  Without argument, this toggles the mode. With a boolean, it
  enables (true) or disables it (false). Given a number, it
  temporarily enables the mode until that number of milliseconds
  have passed or another non-Tab key is pressed.
  */
  setTabFocusMode(e) {
    e == null ? this.inputState.tabFocusMode = this.inputState.tabFocusMode < 0 ? 0 : -1 : typeof e == "boolean" ? this.inputState.tabFocusMode = e ? 0 : -1 : this.inputState.tabFocusMode != 0 && (this.inputState.tabFocusMode = Date.now() + e);
  }
  /**
  Returns an extension that can be used to add DOM event handlers.
  The value should be an object mapping event names to handler
  functions. For any given event, such functions are ordered by
  extension precedence, and the first handler to return true will
  be assumed to have handled that event, and no other handlers or
  built-in behavior will be activated for it. These are registered
  on the [content element](https://codemirror.net/6/docs/ref/#view.EditorView.contentDOM), except
  for `scroll` handlers, which will be called any time the
  editor's [scroll element](https://codemirror.net/6/docs/ref/#view.EditorView.scrollDOM) or one of
  its parent nodes is scrolled.
  */
  static domEventHandlers(e) {
    return De.define(() => ({}), { eventHandlers: e });
  }
  /**
  Create an extension that registers DOM event observers. Contrary
  to event [handlers](https://codemirror.net/6/docs/ref/#view.EditorView^domEventHandlers),
  observers can't be prevented from running by a higher-precedence
  handler returning true. They also don't prevent other handlers
  and observers from running when they return true, and should not
  call `preventDefault`.
  */
  static domEventObservers(e) {
    return De.define(() => ({}), { eventObservers: e });
  }
  /**
  Create a theme extension. The first argument can be a
  [`style-mod`](https://github.com/marijnh/style-mod#documentation)
  style spec providing the styles for the theme. These will be
  prefixed with a generated class for the style.
  
  Because the selectors will be prefixed with a scope class, rule
  that directly match the editor's [wrapper
  element](https://codemirror.net/6/docs/ref/#view.EditorView.dom)—to which the scope class will be
  added—need to be explicitly differentiated by adding an `&` to
  the selector for that element—for example
  `&.cm-focused`.
  
  When `dark` is set to true, the theme will be marked as dark,
  which will cause the `&dark` rules from [base
  themes](https://codemirror.net/6/docs/ref/#view.EditorView^baseTheme) to be used (as opposed to
  `&light` when a light theme is active).
  */
  static theme(e, t) {
    let i = wi.newName(), r = [Rr.of(i), Wn.of(gl(`.${i}`, e))];
    return t && t.dark && r.push(dl.of(!0)), r;
  }
  /**
  Create an extension that adds styles to the base theme. Like
  with [`theme`](https://codemirror.net/6/docs/ref/#view.EditorView^theme), use `&` to indicate the
  place of the editor wrapper element when directly targeting
  that. You can also use `&dark` or `&light` instead to only
  target editors with a dark or light theme.
  */
  static baseTheme(e) {
    return Ti.lowest(Wn.of(gl("." + pl, e, Bu)));
  }
  /**
  Retrieve an editor view instance from the view's DOM
  representation.
  */
  static findFromDOM(e) {
    var t;
    let i = e.querySelector(".cm-content"), r = i && Ne.get(i) || Ne.get(e);
    return ((t = r?.root) === null || t === void 0 ? void 0 : t.view) || null;
  }
}
_.styleModule = Wn;
_.inputHandler = su;
_.clipboardInputFilter = jl;
_.clipboardOutputFilter = Ul;
_.scrollHandler = hu;
_.focusChangeEffect = ou;
_.perLineTextDirection = lu;
_.exceptionSink = ru;
_.updateListener = al;
_.editable = ri;
_.mouseSelectionStyle = nu;
_.dragMovesSelection = iu;
_.clickAddsSelectionRange = tu;
_.decorations = Es;
_.blockWrappers = uu;
_.outerDecorations = Yl;
_.atomicRanges = ur;
_.bidiIsolatedRanges = du;
_.scrollMargins = pu;
_.darkTheme = dl;
_.cspNonce = /* @__PURE__ */ U.define({ combine: (n) => n.length ? n[0] : "" });
_.contentAttributes = Xl;
_.editorAttributes = fu;
_.lineWrapping = /* @__PURE__ */ _.contentAttributes.of({ class: "cm-lineWrapping" });
_.announce = /* @__PURE__ */ ne.define();
const L0 = 4096, Uh = {};
class fs {
  constructor(e, t, i, r, s, o) {
    this.from = e, this.to = t, this.dir = i, this.isolates = r, this.fresh = s, this.order = o;
  }
  static update(e, t) {
    if (t.empty && !e.some((s) => s.fresh))
      return e;
    let i = [], r = e.length ? e[e.length - 1].dir : xe.LTR;
    for (let s = Math.max(0, e.length - 10); s < e.length; s++) {
      let o = e[s];
      o.dir == r && !t.touchesRange(o.from, o.to) && i.push(new fs(t.mapPos(o.from, 1), t.mapPos(o.to, -1), o.dir, o.isolates, !1, o.order));
    }
    return i;
  }
}
function Xh(n, e, t) {
  for (let i = n.state.facet(e), r = i.length - 1; r >= 0; r--) {
    let s = i[r], o = typeof s == "function" ? s(n) : s;
    o && ql(o, t);
  }
  return t;
}
const D0 = j.mac ? "mac" : j.windows ? "win" : j.linux ? "linux" : "key";
function B0(n, e) {
  const t = n.split(/-(?!$)/);
  let i = t[t.length - 1];
  i == "Space" && (i = " ");
  let r, s, o, l;
  for (let a = 0; a < t.length - 1; ++a) {
    const f = t[a];
    if (/^(cmd|meta|m)$/i.test(f))
      l = !0;
    else if (/^a(lt)?$/i.test(f))
      r = !0;
    else if (/^(c|ctrl|control)$/i.test(f))
      s = !0;
    else if (/^s(hift)?$/i.test(f))
      o = !0;
    else if (/^mod$/i.test(f))
      e == "mac" ? l = !0 : s = !0;
    else
      throw new Error("Unrecognized modifier name: " + f);
  }
  return r && (i = "Alt-" + i), s && (i = "Ctrl-" + i), l && (i = "Meta-" + i), o && (i = "Shift-" + i), i;
}
function Lr(n, e, t) {
  return e.altKey && (n = "Alt-" + n), e.ctrlKey && (n = "Ctrl-" + n), e.metaKey && (n = "Meta-" + n), t !== !1 && e.shiftKey && (n = "Shift-" + n), n;
}
const E0 = /* @__PURE__ */ Ti.default(/* @__PURE__ */ _.domEventHandlers({
  keydown(n, e) {
    return Iu(Eu(e.state), n, e, "editor");
  }
})), ta = /* @__PURE__ */ U.define({ enables: E0 }), Yh = /* @__PURE__ */ new WeakMap();
function Eu(n) {
  let e = n.facet(ta), t = Yh.get(e);
  return t || Yh.set(e, t = N0(e.reduce((i, r) => i.concat(r), []))), t;
}
function Ni(n, e, t) {
  return Iu(Eu(n.state), e, n, t);
}
let mi = null;
const I0 = 4e3;
function N0(n, e = D0) {
  let t = /* @__PURE__ */ Object.create(null), i = /* @__PURE__ */ Object.create(null), r = (o, l) => {
    let a = i[o];
    if (a == null)
      i[o] = l;
    else if (a != l)
      throw new Error("Key binding " + o + " is used both as a regular binding and as a multi-stroke prefix");
  }, s = (o, l, a, f, u) => {
    var g, y;
    let b = t[o] || (t[o] = /* @__PURE__ */ Object.create(null)), k = l.split(/ (?!$)/).map((R) => B0(R, e));
    for (let R = 1; R < k.length; R++) {
      let I = k.slice(0, R).join(" ");
      r(I, !0), b[I] || (b[I] = {
        preventDefault: !0,
        stopPropagation: !1,
        run: [(N) => {
          let z = mi = { view: N, prefix: I, scope: o };
          return setTimeout(() => {
            mi == z && (mi = null);
          }, I0), !0;
        }]
      });
    }
    let C = k.join(" ");
    r(C, !1);
    let M = b[C] || (b[C] = {
      preventDefault: !1,
      stopPropagation: !1,
      run: ((y = (g = b._any) === null || g === void 0 ? void 0 : g.run) === null || y === void 0 ? void 0 : y.slice()) || []
    });
    a && M.run.push(a), f && (M.preventDefault = !0), u && (M.stopPropagation = !0);
  };
  for (let o of n) {
    let l = o.scope ? o.scope.split(" ") : ["editor"];
    if (o.any)
      for (let f of l) {
        let u = t[f] || (t[f] = /* @__PURE__ */ Object.create(null));
        u._any || (u._any = { preventDefault: !1, stopPropagation: !1, run: [] });
        let { any: g } = o;
        for (let y in u)
          u[y].run.push((b) => g(b, ml));
      }
    let a = o[e] || o.key;
    if (a)
      for (let f of l)
        s(f, a, o.run, o.preventDefault, o.stopPropagation), o.shift && s(f, "Shift-" + a, o.shift, o.preventDefault, o.stopPropagation);
  }
  return t;
}
let ml = null;
function Iu(n, e, t, i) {
  ml = e;
  let r = Gg(e), s = at(r, 0), o = Yt(s) == r.length && r != " ", l = "", a = !1, f = !1, u = !1;
  mi && mi.view == t && mi.scope == i && (l = mi.prefix + " ", Su.indexOf(e.keyCode) < 0 && (f = !0, mi = null));
  let g = /* @__PURE__ */ new Set(), y = (M) => {
    if (M) {
      for (let R of M.run)
        if (!g.has(R) && (g.add(R), R(t)))
          return M.stopPropagation && (u = !0), !0;
      M.preventDefault && (M.stopPropagation && (u = !0), f = !0);
    }
    return !1;
  }, b = n[i], k, C;
  return b && (y(b[l + Lr(r, e, !o)]) ? a = !0 : o && (e.altKey || e.metaKey || e.ctrlKey) && // Ctrl-Alt may be used for AltGr on Windows
  !(j.windows && e.ctrlKey && e.altKey) && // Alt-combinations on macOS tend to be typed characters
  !(j.mac && e.altKey && !(e.ctrlKey || e.metaKey)) && (k = Si[e.keyCode]) && k != r ? (y(b[l + Lr(k, e, !0)]) || e.shiftKey && (C = Gn[e.keyCode]) != r && C != k && y(b[l + Lr(C, e, !1)])) && (a = !0) : o && e.shiftKey && y(b[l + Lr(r, e, !0)]) && (a = !0), !a && y(b._any) && (a = !0)), f && (a = !0), a && u && e.stopPropagation(), ml = null, a;
}
class dr {
  /**
  Create a marker with the given class and dimensions. If `width`
  is null, the DOM element will get no width style.
  */
  constructor(e, t, i, r, s) {
    this.className = e, this.left = t, this.top = i, this.width = r, this.height = s;
  }
  draw() {
    let e = document.createElement("div");
    return e.className = this.className, this.adjust(e), e;
  }
  update(e, t) {
    return t.className != this.className ? !1 : (this.adjust(e), !0);
  }
  adjust(e) {
    e.style.left = this.left + "px", e.style.top = this.top + "px", this.width != null && (e.style.width = this.width + "px"), e.style.height = this.height + "px";
  }
  eq(e) {
    return this.left == e.left && this.top == e.top && this.width == e.width && this.height == e.height && this.className == e.className;
  }
  /**
  Create a set of rectangles for the given selection range,
  assigning them theclass`className`. Will create a single
  rectangle for empty ranges, and a set of selection-style
  rectangles covering the range's content (in a bidi-aware
  way) for non-empty ones.
  */
  static forRange(e, t, i) {
    if (i.empty) {
      let r = e.coordsAtPos(i.head, i.assoc || 1);
      if (!r)
        return [];
      let s = Nu(e);
      return [new dr(t, r.left - s.left, r.top - s.top, null, r.bottom - r.top)];
    } else
      return F0(e, t, i);
  }
}
function Nu(n) {
  let e = n.scrollDOM.getBoundingClientRect();
  return { left: (n.textDirection == xe.LTR ? e.left : e.right - n.scrollDOM.clientWidth * n.scaleX) - n.scrollDOM.scrollLeft * n.scaleX, top: e.top - n.scrollDOM.scrollTop * n.scaleY };
}
function Gh(n, e, t, i) {
  let r = n.coordsAtPos(e, t * 2);
  if (!r)
    return i;
  let s = n.dom.getBoundingClientRect(), o = (r.top + r.bottom) / 2, l = n.posAtCoords({ x: s.left + 1, y: o }), a = n.posAtCoords({ x: s.right - 1, y: o });
  return l == null || a == null ? i : { from: Math.max(i.from, Math.min(l, a)), to: Math.min(i.to, Math.max(l, a)) };
}
function F0(n, e, t) {
  if (t.to <= n.viewport.from || t.from >= n.viewport.to)
    return [];
  let i = Math.max(t.from, n.viewport.from), r = Math.min(t.to, n.viewport.to), s = n.textDirection == xe.LTR, o = n.contentDOM, l = o.getBoundingClientRect(), a = Nu(n), f = o.querySelector(".cm-line"), u = f && window.getComputedStyle(f), g = l.left + (u ? parseInt(u.paddingLeft) + Math.min(0, parseInt(u.textIndent)) : 0), y = l.right - (u ? parseInt(u.paddingRight) : 0), b = cl(n, i, 1), k = cl(n, r, -1), C = b.type == Ye.Text ? b : null, M = k.type == Ye.Text ? k : null;
  if (C && (n.lineWrapping || b.widgetLineBreaks) && (C = Gh(n, i, 1, C)), M && (n.lineWrapping || k.widgetLineBreaks) && (M = Gh(n, r, -1, M)), C && M && C.from == M.from && C.to == M.to)
    return I(N(t.from, t.to, C));
  {
    let F = C ? N(t.from, null, C) : z(b, !1), H = M ? N(null, t.to, M) : z(k, !0), Q = [];
    return (C || b).to < (M || k).from - (C && M ? 1 : 0) || b.widgetLineBreaks > 1 && F.bottom + n.defaultLineHeight / 2 < H.top ? Q.push(R(g, F.bottom, y, H.top)) : F.bottom < H.top && n.elementAtHeight((F.bottom + H.top) / 2).type == Ye.Text && (F.bottom = H.top = (F.bottom + H.top) / 2), I(F).concat(Q).concat(I(H));
  }
  function R(F, H, Q, Z) {
    return new dr(e, F - a.left, H - a.top, Q - F, Z - H);
  }
  function I({ top: F, bottom: H, horizontal: Q }) {
    let Z = [];
    for (let le = 0; le < Q.length; le += 2)
      Z.push(R(Q[le], F, Q[le + 1], H));
    return Z;
  }
  function N(F, H, Q) {
    let Z = 1e9, le = -1e9, he = [];
    function ee(fe, me, qe, Be, q) {
      let Ee = n.coordsAtPos(fe, fe == Q.to ? -2 : 2), Ge = n.coordsAtPos(qe, qe == Q.from ? 2 : -2);
      !Ee || !Ge || (Z = Math.min(Ee.top, Ge.top, Z), le = Math.max(Ee.bottom, Ge.bottom, le), q == xe.LTR ? he.push(s && me ? g : Ee.left, s && Be ? y : Ge.right) : he.push(!s && Be ? g : Ge.left, !s && me ? y : Ee.right));
    }
    let Y = F ?? Q.from, ie = H ?? Q.to;
    for (let fe of n.visibleRanges)
      if (fe.to > Y && fe.from < ie)
        for (let me = Math.max(fe.from, Y), qe = Math.min(fe.to, ie); ; ) {
          let Be = n.state.doc.lineAt(me);
          for (let q of n.bidiSpans(Be)) {
            let Ee = q.from + Be.from, Ge = q.to + Be.from;
            if (Ee >= qe)
              break;
            Ge > me && ee(Math.max(Ee, me), F == null && Ee <= Y, Math.min(Ge, qe), H == null && Ge >= ie, q.dir);
          }
          if (me = Be.to + 1, me >= qe)
            break;
        }
    return he.length == 0 && ee(Y, F == null, ie, H == null, n.textDirection), { top: Z, bottom: le, horizontal: he };
  }
  function z(F, H) {
    let Q = l.top + (H ? F.top : F.bottom);
    return { top: Q, bottom: Q, horizontal: [] };
  }
}
function W0(n, e) {
  return n.constructor == e.constructor && n.eq(e);
}
class Q0 {
  constructor(e, t) {
    this.view = e, this.layer = t, this.drawn = [], this.scaleX = 1, this.scaleY = 1, this.measureReq = { read: this.measure.bind(this), write: this.draw.bind(this) }, this.dom = e.scrollDOM.appendChild(document.createElement("div")), this.dom.classList.add("cm-layer"), t.above && this.dom.classList.add("cm-layer-above"), t.class && this.dom.classList.add(t.class), this.scale(), this.dom.setAttribute("aria-hidden", "true"), this.setOrder(e.state), e.requestMeasure(this.measureReq), t.mount && t.mount(this.dom, e);
  }
  update(e) {
    e.startState.facet(Xr) != e.state.facet(Xr) && this.setOrder(e.state), (this.layer.update(e, this.dom) || e.geometryChanged) && (this.scale(), e.view.requestMeasure(this.measureReq));
  }
  docViewUpdate(e) {
    this.layer.updateOnDocViewUpdate !== !1 && e.requestMeasure(this.measureReq);
  }
  setOrder(e) {
    let t = 0, i = e.facet(Xr);
    for (; t < i.length && i[t] != this.layer; )
      t++;
    this.dom.style.zIndex = String((this.layer.above ? 150 : -1) - t);
  }
  measure() {
    return this.layer.markers(this.view);
  }
  scale() {
    let { scaleX: e, scaleY: t } = this.view;
    (e != this.scaleX || t != this.scaleY) && (this.scaleX = e, this.scaleY = t, this.dom.style.transform = `scale(${1 / e}, ${1 / t})`);
  }
  draw(e) {
    if (e.length != this.drawn.length || e.some((t, i) => !W0(t, this.drawn[i]))) {
      let t = this.dom.firstChild, i = 0;
      for (let r of e)
        r.update && t && r.constructor && this.drawn[i].constructor && r.update(t, this.drawn[i]) ? (t = t.nextSibling, i++) : this.dom.insertBefore(r.draw(), t);
      for (; t; ) {
        let r = t.nextSibling;
        t.remove(), t = r;
      }
      this.drawn = e, j.safari && j.safari_version >= 26 && (this.dom.style.display = this.dom.firstChild ? "" : "none");
    }
  }
  destroy() {
    this.layer.destroy && this.layer.destroy(this.dom, this.view), this.dom.remove();
  }
}
const Xr = /* @__PURE__ */ U.define();
function Fu(n) {
  return [
    De.define((e) => new Q0(e, n)),
    Xr.of(n)
  ];
}
const wn = /* @__PURE__ */ U.define({
  combine(n) {
    return ti(n, {
      cursorBlinkRate: 1200,
      drawRangeCursor: !0
    }, {
      cursorBlinkRate: (e, t) => Math.min(e, t),
      drawRangeCursor: (e, t) => e || t
    });
  }
});
function V0(n = {}) {
  return [
    wn.of(n),
    z0,
    $0,
    q0,
    au.of(!0)
  ];
}
function H0(n) {
  return n.facet(wn);
}
function Wu(n) {
  return n.startState.facet(wn) != n.state.facet(wn);
}
const z0 = /* @__PURE__ */ Fu({
  above: !0,
  markers(n) {
    let { state: e } = n, t = e.facet(wn), i = [];
    for (let r of e.selection.ranges) {
      let s = r == e.selection.main;
      if (r.empty || t.drawRangeCursor) {
        let o = s ? "cm-cursor cm-cursor-primary" : "cm-cursor cm-cursor-secondary", l = r.empty ? r : E.cursor(r.head, r.head > r.anchor ? -1 : 1);
        for (let a of dr.forRange(n, o, l))
          i.push(a);
      }
    }
    return i;
  },
  update(n, e) {
    n.transactions.some((i) => i.selection) && (e.style.animationName = e.style.animationName == "cm-blink" ? "cm-blink2" : "cm-blink");
    let t = Wu(n);
    return t && Zh(n.state, e), n.docChanged || n.selectionSet || t;
  },
  mount(n, e) {
    Zh(e.state, n);
  },
  class: "cm-cursorLayer"
});
function Zh(n, e) {
  e.style.animationDuration = n.facet(wn).cursorBlinkRate + "ms";
}
const $0 = /* @__PURE__ */ Fu({
  above: !1,
  markers(n) {
    return n.state.selection.ranges.map((e) => e.empty ? [] : dr.forRange(n, "cm-selectionBackground", e)).reduce((e, t) => e.concat(t));
  },
  update(n, e) {
    return n.docChanged || n.selectionSet || n.viewportChanged || Wu(n);
  },
  class: "cm-selectionLayer"
}), q0 = /* @__PURE__ */ Ti.highest(/* @__PURE__ */ _.theme({
  ".cm-line": {
    "& ::selection, &::selection": { backgroundColor: "transparent !important" },
    caretColor: "transparent !important"
  },
  ".cm-content": {
    caretColor: "transparent !important",
    "& :focus": {
      caretColor: "initial !important",
      "&::selection, & ::selection": {
        backgroundColor: "Highlight !important"
      }
    }
  }
})), Qu = /* @__PURE__ */ ne.define({
  map(n, e) {
    return n == null ? null : e.mapPos(n);
  }
}), Hn = /* @__PURE__ */ $e.define({
  create() {
    return null;
  },
  update(n, e) {
    return n != null && (n = e.changes.mapPos(n)), e.effects.reduce((t, i) => i.is(Qu) ? i.value : t, n);
  }
}), K0 = /* @__PURE__ */ De.fromClass(class {
  constructor(n) {
    this.view = n, this.cursor = null, this.measureReq = { read: this.readPos.bind(this), write: this.drawCursor.bind(this) };
  }
  update(n) {
    var e;
    let t = n.state.field(Hn);
    t == null ? this.cursor != null && ((e = this.cursor) === null || e === void 0 || e.remove(), this.cursor = null) : (this.cursor || (this.cursor = this.view.scrollDOM.appendChild(document.createElement("div")), this.cursor.className = "cm-dropCursor"), (n.startState.field(Hn) != t || n.docChanged || n.geometryChanged) && this.view.requestMeasure(this.measureReq));
  }
  readPos() {
    let { view: n } = this, e = n.state.field(Hn), t = e != null && n.coordsAtPos(e);
    if (!t)
      return null;
    let i = n.scrollDOM.getBoundingClientRect();
    return {
      left: t.left - i.left + n.scrollDOM.scrollLeft * n.scaleX,
      top: t.top - i.top + n.scrollDOM.scrollTop * n.scaleY,
      height: t.bottom - t.top
    };
  }
  drawCursor(n) {
    if (this.cursor) {
      let { scaleX: e, scaleY: t } = this.view;
      n ? (this.cursor.style.left = n.left / e + "px", this.cursor.style.top = n.top / t + "px", this.cursor.style.height = n.height / t + "px") : this.cursor.style.left = "-100000px";
    }
  }
  destroy() {
    this.cursor && this.cursor.remove();
  }
  setDropPos(n) {
    this.view.state.field(Hn) != n && this.view.dispatch({ effects: Qu.of(n) });
  }
}, {
  eventObservers: {
    dragover(n) {
      this.setDropPos(this.view.posAtCoords({ x: n.clientX, y: n.clientY }));
    },
    dragleave(n) {
      (n.target == this.view.contentDOM || !this.view.contentDOM.contains(n.relatedTarget)) && this.setDropPos(null);
    },
    dragend() {
      this.setDropPos(null);
    },
    drop() {
      this.setDropPos(null);
    }
  }
});
function _0() {
  return [Hn, K0];
}
function Jh(n, e, t, i, r) {
  e.lastIndex = 0;
  for (let s = n.iterRange(t, i), o = t, l; !s.next().done; o += s.value.length)
    if (!s.lineBreak)
      for (; l = e.exec(s.value); )
        r(o + l.index, l);
}
function j0(n, e) {
  let t = n.visibleRanges;
  if (t.length == 1 && t[0].from == n.viewport.from && t[0].to == n.viewport.to)
    return t;
  let i = [];
  for (let { from: r, to: s } of t)
    r = Math.max(n.state.doc.lineAt(r).from, r - e), s = Math.min(n.state.doc.lineAt(s).to, s + e), i.length && i[i.length - 1].to >= r ? i[i.length - 1].to = s : i.push({ from: r, to: s });
  return i;
}
class U0 {
  /**
  Create a decorator.
  */
  constructor(e) {
    const { regexp: t, decoration: i, decorate: r, boundary: s, maxLength: o = 1e3 } = e;
    if (!t.global)
      throw new RangeError("The regular expression given to MatchDecorator should have its 'g' flag set");
    if (this.regexp = t, r)
      this.addMatch = (l, a, f, u) => r(u, f, f + l[0].length, l, a);
    else if (typeof i == "function")
      this.addMatch = (l, a, f, u) => {
        let g = i(l, a, f);
        g && u(f, f + l[0].length, g);
      };
    else if (i)
      this.addMatch = (l, a, f, u) => u(f, f + l[0].length, i);
    else
      throw new RangeError("Either 'decorate' or 'decoration' should be provided to MatchDecorator");
    this.boundary = s, this.maxLength = o;
  }
  /**
  Compute the full set of decorations for matches in the given
  view's viewport. You'll want to call this when initializing your
  plugin.
  */
  createDeco(e) {
    let t = new ei(), i = t.add.bind(t);
    for (let { from: r, to: s } of j0(e, this.maxLength))
      Jh(e.state.doc, this.regexp, r, s, (o, l) => this.addMatch(l, e, o, i));
    return t.finish();
  }
  /**
  Update a set of decorations for a view update. `deco` _must_ be
  the set of decorations produced by _this_ `MatchDecorator` for
  the view state before the update.
  */
  updateDeco(e, t) {
    let i = 1e9, r = -1;
    return e.docChanged && e.changes.iterChanges((s, o, l, a) => {
      a >= e.view.viewport.from && l <= e.view.viewport.to && (i = Math.min(l, i), r = Math.max(a, r));
    }), e.viewportMoved || r - i > 1e3 ? this.createDeco(e.view) : r > -1 ? this.updateRange(e.view, t.map(e.changes), i, r) : t;
  }
  updateRange(e, t, i, r) {
    for (let s of e.visibleRanges) {
      let o = Math.max(s.from, i), l = Math.min(s.to, r);
      if (l >= o) {
        let a = e.state.doc.lineAt(o), f = a.to < l ? e.state.doc.lineAt(l) : a, u = Math.max(s.from, a.from), g = Math.min(s.to, f.to);
        if (this.boundary) {
          for (; o > a.from; o--)
            if (this.boundary.test(a.text[o - 1 - a.from])) {
              u = o;
              break;
            }
          for (; l < f.to; l++)
            if (this.boundary.test(f.text[l - f.from])) {
              g = l;
              break;
            }
        }
        let y = [], b, k = (C, M, R) => y.push(R.range(C, M));
        if (a == f)
          for (this.regexp.lastIndex = u - a.from; (b = this.regexp.exec(a.text)) && b.index < g - a.from; )
            this.addMatch(b, e, b.index + a.from, k);
        else
          Jh(e.state.doc, this.regexp, u, g, (C, M) => this.addMatch(M, e, C, k));
        t = t.update({ filterFrom: u, filterTo: g, filter: (C, M) => C < u || M > g, add: y });
      }
    }
    return t;
  }
}
const vl = /x/.unicode != null ? "gu" : "g", X0 = /* @__PURE__ */ new RegExp(`[\0-\b
--­؜​‎‏\u2028\u2029‭‮⁦⁧⁩\uFEFF￹-￼]`, vl), Y0 = {
  0: "null",
  7: "bell",
  8: "backspace",
  10: "newline",
  11: "vertical tab",
  13: "carriage return",
  27: "escape",
  8203: "zero width space",
  8204: "zero width non-joiner",
  8205: "zero width joiner",
  8206: "left-to-right mark",
  8207: "right-to-left mark",
  8232: "line separator",
  8237: "left-to-right override",
  8238: "right-to-left override",
  8294: "left-to-right isolate",
  8295: "right-to-left isolate",
  8297: "pop directional isolate",
  8233: "paragraph separator",
  65279: "zero width no-break space",
  65532: "object replacement"
};
let yo = null;
function G0() {
  var n;
  if (yo == null && typeof document < "u" && document.body) {
    let e = document.body.style;
    yo = ((n = e.tabSize) !== null && n !== void 0 ? n : e.MozTabSize) != null;
  }
  return yo || !1;
}
const Yr = /* @__PURE__ */ U.define({
  combine(n) {
    let e = ti(n, {
      render: null,
      specialChars: X0,
      addSpecialChars: null
    });
    return (e.replaceTabs = !G0()) && (e.specialChars = new RegExp("	|" + e.specialChars.source, vl)), e.addSpecialChars && (e.specialChars = new RegExp(e.specialChars.source + "|" + e.addSpecialChars.source, vl)), e;
  }
});
function Z0(n = {}) {
  return [Yr.of(n), J0()];
}
let ec = null;
function J0() {
  return ec || (ec = De.fromClass(class {
    constructor(n) {
      this.view = n, this.decorations = G.none, this.decorationCache = /* @__PURE__ */ Object.create(null), this.decorator = this.makeDecorator(n.state.facet(Yr)), this.decorations = this.decorator.createDeco(n);
    }
    makeDecorator(n) {
      return new U0({
        regexp: n.specialChars,
        decoration: (e, t, i) => {
          let { doc: r } = t.state, s = at(e[0], 0);
          if (s == 9) {
            let o = r.lineAt(i), l = t.state.tabSize, a = Mn(o.text, l, i - o.from);
            return G.replace({
              widget: new nv((l - a % l) * this.view.defaultCharacterWidth / this.view.scaleX)
            });
          }
          return this.decorationCache[s] || (this.decorationCache[s] = G.replace({ widget: new iv(n, s) }));
        },
        boundary: n.replaceTabs ? void 0 : /[^]/
      });
    }
    update(n) {
      let e = n.state.facet(Yr);
      n.startState.facet(Yr) != e ? (this.decorator = this.makeDecorator(e), this.decorations = this.decorator.createDeco(n.view)) : this.decorations = this.decorator.updateDeco(n, this.decorations);
    }
  }, {
    decorations: (n) => n.decorations
  }));
}
const ev = "•";
function tv(n) {
  return n >= 32 ? ev : n == 10 ? "␤" : String.fromCharCode(9216 + n);
}
class iv extends ci {
  constructor(e, t) {
    super(), this.options = e, this.code = t;
  }
  eq(e) {
    return e.code == this.code;
  }
  toDOM(e) {
    let t = tv(this.code), i = e.state.phrase("Control character") + " " + (Y0[this.code] || "0x" + this.code.toString(16)), r = this.options.render && this.options.render(this.code, i, t);
    if (r)
      return r;
    let s = document.createElement("span");
    return s.textContent = t, s.title = i, s.setAttribute("aria-label", i), s.className = "cm-specialChar", s;
  }
  ignoreEvent() {
    return !1;
  }
}
class nv extends ci {
  constructor(e) {
    super(), this.width = e;
  }
  eq(e) {
    return e.width == this.width;
  }
  toDOM() {
    let e = document.createElement("span");
    return e.textContent = "	", e.className = "cm-tab", e.style.width = this.width + "px", e;
  }
  ignoreEvent() {
    return !1;
  }
}
function rv() {
  return ov;
}
const sv = /* @__PURE__ */ G.line({ class: "cm-activeLine" }), ov = /* @__PURE__ */ De.fromClass(class {
  constructor(n) {
    this.decorations = this.getDeco(n);
  }
  update(n) {
    (n.docChanged || n.selectionSet) && (this.decorations = this.getDeco(n.view));
  }
  getDeco(n) {
    let e = -1, t = [];
    for (let i of n.state.selection.ranges) {
      let r = n.lineBlockAt(i.head);
      r.from > e && (t.push(sv.range(r.from)), e = r.from);
    }
    return G.set(t);
  }
}, {
  decorations: (n) => n.decorations
}), yl = 2e3;
function lv(n, e, t) {
  let i = Math.min(e.line, t.line), r = Math.max(e.line, t.line), s = [];
  if (e.off > yl || t.off > yl || e.col < 0 || t.col < 0) {
    let o = Math.min(e.off, t.off), l = Math.max(e.off, t.off);
    for (let a = i; a <= r; a++) {
      let f = n.doc.line(a);
      f.length <= l && s.push(E.range(f.from + o, f.to + l));
    }
  } else {
    let o = Math.min(e.col, t.col), l = Math.max(e.col, t.col);
    for (let a = i; a <= r; a++) {
      let f = n.doc.line(a), u = Go(f.text, o, n.tabSize, !0);
      if (u < 0)
        s.push(E.cursor(f.to));
      else {
        let g = Go(f.text, l, n.tabSize);
        s.push(E.range(f.from + u, f.from + g));
      }
    }
  }
  return s;
}
function av(n, e) {
  let t = n.coordsAtPos(n.viewport.from);
  return t ? Math.round(Math.abs((t.left - e) / n.defaultCharacterWidth)) : -1;
}
function tc(n, e) {
  let t = n.posAtCoords({ x: e.clientX, y: e.clientY }, !1), i = n.state.doc.lineAt(t), r = t - i.from, s = r > yl ? -1 : r == i.length ? av(n, e.clientX) : Mn(i.text, n.state.tabSize, t - i.from);
  return { line: i.number, col: s, off: r };
}
function hv(n, e) {
  let t = tc(n, e), i = n.state.selection;
  return t ? {
    update(r) {
      if (r.docChanged) {
        let s = r.changes.mapPos(r.startState.doc.line(t.line).from), o = r.state.doc.lineAt(s);
        t = { line: o.number, col: t.col, off: Math.min(t.off, o.length) }, i = i.map(r.changes);
      }
    },
    get(r, s, o) {
      let l = tc(n, r);
      if (!l)
        return i;
      let a = lv(n.state, t, l);
      return a.length ? o ? E.create(a.concat(i.ranges)) : E.create(a) : i;
    }
  } : null;
}
function cv(n) {
  let e = ((t) => t.altKey && t.button == 0);
  return _.mouseSelectionStyle.of((t, i) => e(i) ? hv(t, i) : null);
}
const fv = {
  Alt: [18, (n) => !!n.altKey],
  Control: [17, (n) => !!n.ctrlKey],
  Shift: [16, (n) => !!n.shiftKey],
  Meta: [91, (n) => !!n.metaKey]
}, uv = { style: "cursor: crosshair" };
function dv(n = {}) {
  let [e, t] = fv[n.key || "Alt"], i = De.fromClass(class {
    constructor(r) {
      this.view = r, this.isDown = !1;
    }
    set(r) {
      this.isDown != r && (this.isDown = r, this.view.update([]));
    }
  }, {
    eventObservers: {
      keydown(r) {
        this.set(r.keyCode == e || t(r));
      },
      keyup(r) {
        (r.keyCode == e || !t(r)) && this.set(!1);
      },
      mousemove(r) {
        this.set(t(r));
      }
    }
  });
  return [
    i,
    _.contentAttributes.of((r) => {
      var s;
      return !((s = r.plugin(i)) === null || s === void 0) && s.isDown ? uv : null;
    })
  ];
}
const Dr = "-10000px";
class Vu {
  constructor(e, t, i, r) {
    this.facet = t, this.createTooltipView = i, this.removeTooltipView = r, this.input = e.state.facet(t), this.tooltips = this.input.filter((o) => o);
    let s = null;
    this.tooltipViews = this.tooltips.map((o) => s = i(o, s));
  }
  update(e, t) {
    var i;
    let r = e.state.facet(this.facet), s = r.filter((a) => a);
    if (r === this.input) {
      for (let a of this.tooltipViews)
        a.update && a.update(e);
      return !1;
    }
    let o = [], l = t ? [] : null;
    for (let a = 0; a < s.length; a++) {
      let f = s[a], u = -1;
      if (f) {
        for (let g = 0; g < this.tooltips.length; g++) {
          let y = this.tooltips[g];
          y && y.create == f.create && (u = g);
        }
        if (u < 0)
          o[a] = this.createTooltipView(f, a ? o[a - 1] : null), l && (l[a] = !!f.above);
        else {
          let g = o[a] = this.tooltipViews[u];
          l && (l[a] = t[u]), g.update && g.update(e);
        }
      }
    }
    for (let a of this.tooltipViews)
      o.indexOf(a) < 0 && (this.removeTooltipView(a), (i = a.destroy) === null || i === void 0 || i.call(a));
    return t && (l.forEach((a, f) => t[f] = a), t.length = l.length), this.input = r, this.tooltips = s, this.tooltipViews = o, !0;
  }
}
function pv(n) {
  let e = n.dom.ownerDocument.documentElement;
  return { top: 0, left: 0, bottom: e.clientHeight, right: e.clientWidth };
}
const bo = /* @__PURE__ */ U.define({
  combine: (n) => {
    var e, t, i;
    return {
      position: j.ios ? "absolute" : ((e = n.find((r) => r.position)) === null || e === void 0 ? void 0 : e.position) || "fixed",
      parent: ((t = n.find((r) => r.parent)) === null || t === void 0 ? void 0 : t.parent) || null,
      tooltipSpace: ((i = n.find((r) => r.tooltipSpace)) === null || i === void 0 ? void 0 : i.tooltipSpace) || pv
    };
  }
}), ic = /* @__PURE__ */ new WeakMap(), ia = /* @__PURE__ */ De.fromClass(class {
  constructor(n) {
    this.view = n, this.above = [], this.inView = !0, this.madeAbsolute = !1, this.lastTransaction = 0, this.measureTimeout = -1;
    let e = n.state.facet(bo);
    this.position = e.position, this.parent = e.parent, this.classes = n.themeClasses, this.createContainer(), this.measureReq = { read: this.readMeasure.bind(this), write: this.writeMeasure.bind(this), key: this }, this.resizeObserver = typeof ResizeObserver == "function" ? new ResizeObserver(() => this.measureSoon()) : null, this.manager = new Vu(n, na, (t, i) => this.createTooltip(t, i), (t) => {
      this.resizeObserver && this.resizeObserver.unobserve(t.dom), t.dom.remove();
    }), this.above = this.manager.tooltips.map((t) => !!t.above), this.intersectionObserver = typeof IntersectionObserver == "function" ? new IntersectionObserver((t) => {
      Date.now() > this.lastTransaction - 50 && t.length > 0 && t[t.length - 1].intersectionRatio < 1 && this.measureSoon();
    }, { threshold: [1] }) : null, this.observeIntersection(), n.win.addEventListener("resize", this.measureSoon = this.measureSoon.bind(this)), this.maybeMeasure();
  }
  createContainer() {
    this.parent ? (this.container = document.createElement("div"), this.container.style.position = "relative", this.container.className = this.view.themeClasses, this.parent.appendChild(this.container)) : this.container = this.view.dom;
  }
  observeIntersection() {
    if (this.intersectionObserver) {
      this.intersectionObserver.disconnect();
      for (let n of this.manager.tooltipViews)
        this.intersectionObserver.observe(n.dom);
    }
  }
  measureSoon() {
    this.measureTimeout < 0 && (this.measureTimeout = setTimeout(() => {
      this.measureTimeout = -1, this.maybeMeasure();
    }, 50));
  }
  update(n) {
    n.transactions.length && (this.lastTransaction = Date.now());
    let e = this.manager.update(n, this.above);
    e && this.observeIntersection();
    let t = e || n.geometryChanged, i = n.state.facet(bo);
    if (i.position != this.position && !this.madeAbsolute) {
      this.position = i.position;
      for (let r of this.manager.tooltipViews)
        r.dom.style.position = this.position;
      t = !0;
    }
    if (i.parent != this.parent) {
      this.parent && this.container.remove(), this.parent = i.parent, this.createContainer();
      for (let r of this.manager.tooltipViews)
        this.container.appendChild(r.dom);
      t = !0;
    } else this.parent && this.view.themeClasses != this.classes && (this.classes = this.container.className = this.view.themeClasses);
    t && this.maybeMeasure();
  }
  createTooltip(n, e) {
    let t = n.create(this.view), i = e ? e.dom : null;
    if (t.dom.classList.add("cm-tooltip"), n.arrow && !t.dom.querySelector(".cm-tooltip > .cm-tooltip-arrow")) {
      let r = document.createElement("div");
      r.className = "cm-tooltip-arrow", t.dom.appendChild(r);
    }
    return t.dom.style.position = this.position, t.dom.style.top = Dr, t.dom.style.left = "0px", this.container.insertBefore(t.dom, i), t.mount && t.mount(this.view), this.resizeObserver && this.resizeObserver.observe(t.dom), t;
  }
  destroy() {
    var n, e, t;
    this.view.win.removeEventListener("resize", this.measureSoon);
    for (let i of this.manager.tooltipViews)
      i.dom.remove(), (n = i.destroy) === null || n === void 0 || n.call(i);
    this.parent && this.container.remove(), (e = this.resizeObserver) === null || e === void 0 || e.disconnect(), (t = this.intersectionObserver) === null || t === void 0 || t.disconnect(), clearTimeout(this.measureTimeout);
  }
  readMeasure() {
    let n = 1, e = 1, t = !1;
    if (this.position == "fixed" && this.manager.tooltipViews.length) {
      let { dom: s } = this.manager.tooltipViews[0];
      if (j.safari) {
        let o = s.getBoundingClientRect();
        t = Math.abs(o.top + 1e4) > 1 || Math.abs(o.left) > 1;
      } else
        t = !!s.offsetParent && s.offsetParent != this.container.ownerDocument.body;
    }
    if (t || this.position == "absolute")
      if (this.parent) {
        let s = this.parent.getBoundingClientRect();
        s.width && s.height && (n = s.width / this.parent.offsetWidth, e = s.height / this.parent.offsetHeight);
      } else
        ({ scaleX: n, scaleY: e } = this.view.viewState);
    let i = this.view.scrollDOM.getBoundingClientRect(), r = Gl(this.view);
    return {
      visible: {
        left: i.left + r.left,
        top: i.top + r.top,
        right: i.right - r.right,
        bottom: i.bottom - r.bottom
      },
      parent: this.parent ? this.container.getBoundingClientRect() : this.view.dom.getBoundingClientRect(),
      pos: this.manager.tooltips.map((s, o) => {
        let l = this.manager.tooltipViews[o];
        return l.getCoords ? l.getCoords(s.pos) : this.view.coordsAtPos(s.pos);
      }),
      size: this.manager.tooltipViews.map(({ dom: s }) => s.getBoundingClientRect()),
      space: this.view.state.facet(bo).tooltipSpace(this.view),
      scaleX: n,
      scaleY: e,
      makeAbsolute: t
    };
  }
  writeMeasure(n) {
    var e;
    if (n.makeAbsolute) {
      this.madeAbsolute = !0, this.position = "absolute";
      for (let l of this.manager.tooltipViews)
        l.dom.style.position = "absolute";
    }
    let { visible: t, space: i, scaleX: r, scaleY: s } = n, o = [];
    for (let l = 0; l < this.manager.tooltips.length; l++) {
      let a = this.manager.tooltips[l], f = this.manager.tooltipViews[l], { dom: u } = f, g = n.pos[l], y = n.size[l];
      if (!g || a.clip !== !1 && (g.bottom <= Math.max(t.top, i.top) || g.top >= Math.min(t.bottom, i.bottom) || g.right < Math.max(t.left, i.left) - 0.1 || g.left > Math.min(t.right, i.right) + 0.1)) {
        u.style.top = Dr;
        continue;
      }
      let b = a.arrow ? f.dom.querySelector(".cm-tooltip-arrow") : null, k = b ? 7 : 0, C = y.right - y.left, M = (e = ic.get(f)) !== null && e !== void 0 ? e : y.bottom - y.top, R = f.offset || mv, I = this.view.textDirection == xe.LTR, N = y.width > i.right - i.left ? I ? i.left : i.right - y.width : I ? Math.max(i.left, Math.min(g.left - (b ? 14 : 0) + R.x, i.right - C)) : Math.min(Math.max(i.left, g.left - C + (b ? 14 : 0) - R.x), i.right - C), z = this.above[l];
      !a.strictSide && (z ? g.top - M - k - R.y < i.top : g.bottom + M + k + R.y > i.bottom) && z == i.bottom - g.bottom > g.top - i.top && (z = this.above[l] = !z);
      let F = (z ? g.top - i.top : i.bottom - g.bottom) - k;
      if (F < M && f.resize !== !1) {
        if (F < this.view.defaultLineHeight) {
          u.style.top = Dr;
          continue;
        }
        ic.set(f, M), u.style.height = (M = F) / s + "px";
      } else u.style.height && (u.style.height = "");
      let H = z ? g.top - M - k - R.y : g.bottom + k + R.y, Q = N + C;
      if (f.overlap !== !0)
        for (let Z of o)
          Z.left < Q && Z.right > N && Z.top < H + M && Z.bottom > H && (H = z ? Z.top - M - 2 - k : Z.bottom + k + 2);
      if (this.position == "absolute" ? (u.style.top = (H - n.parent.top) / s + "px", nc(u, (N - n.parent.left) / r)) : (u.style.top = H / s + "px", nc(u, N / r)), b) {
        let Z = g.left + (I ? R.x : -R.x) - (N + 14 - 7);
        b.style.left = Z / r + "px";
      }
      f.overlap !== !0 && o.push({ left: N, top: H, right: Q, bottom: H + M }), u.classList.toggle("cm-tooltip-above", z), u.classList.toggle("cm-tooltip-below", !z), f.positioned && f.positioned(n.space);
    }
  }
  maybeMeasure() {
    if (this.manager.tooltips.length && (this.view.inView && this.view.requestMeasure(this.measureReq), this.inView != this.view.inView && (this.inView = this.view.inView, !this.inView)))
      for (let n of this.manager.tooltipViews)
        n.dom.style.top = Dr;
  }
}, {
  eventObservers: {
    scroll() {
      this.maybeMeasure();
    }
  }
});
function nc(n, e) {
  let t = parseInt(n.style.left, 10);
  (isNaN(t) || Math.abs(e - t) > 1) && (n.style.left = e + "px");
}
const gv = /* @__PURE__ */ _.baseTheme({
  ".cm-tooltip": {
    zIndex: 500,
    boxSizing: "border-box"
  },
  "&light .cm-tooltip": {
    border: "1px solid #bbb",
    backgroundColor: "#f5f5f5"
  },
  "&light .cm-tooltip-section:not(:first-child)": {
    borderTop: "1px solid #bbb"
  },
  "&dark .cm-tooltip": {
    backgroundColor: "#333338",
    color: "white"
  },
  ".cm-tooltip-arrow": {
    height: "7px",
    width: "14px",
    position: "absolute",
    zIndex: -1,
    overflow: "hidden",
    "&:before, &:after": {
      content: "''",
      position: "absolute",
      width: 0,
      height: 0,
      borderLeft: "7px solid transparent",
      borderRight: "7px solid transparent"
    },
    ".cm-tooltip-above &": {
      bottom: "-7px",
      "&:before": {
        borderTop: "7px solid #bbb"
      },
      "&:after": {
        borderTop: "7px solid #f5f5f5",
        bottom: "1px"
      }
    },
    ".cm-tooltip-below &": {
      top: "-7px",
      "&:before": {
        borderBottom: "7px solid #bbb"
      },
      "&:after": {
        borderBottom: "7px solid #f5f5f5",
        top: "1px"
      }
    }
  },
  "&dark .cm-tooltip .cm-tooltip-arrow": {
    "&:before": {
      borderTopColor: "#333338",
      borderBottomColor: "#333338"
    },
    "&:after": {
      borderTopColor: "transparent",
      borderBottomColor: "transparent"
    }
  }
}), mv = { x: 0, y: 0 }, na = /* @__PURE__ */ U.define({
  enables: [ia, gv]
}), us = /* @__PURE__ */ U.define({
  combine: (n) => n.reduce((e, t) => e.concat(t), [])
});
class Ws {
  // Needs to be static so that host tooltip instances always match
  static create(e) {
    return new Ws(e);
  }
  constructor(e) {
    this.view = e, this.mounted = !1, this.dom = document.createElement("div"), this.dom.classList.add("cm-tooltip-hover"), this.manager = new Vu(e, us, (t, i) => this.createHostedView(t, i), (t) => t.dom.remove());
  }
  createHostedView(e, t) {
    let i = e.create(this.view);
    return i.dom.classList.add("cm-tooltip-section"), this.dom.insertBefore(i.dom, t ? t.dom.nextSibling : this.dom.firstChild), this.mounted && i.mount && i.mount(this.view), i;
  }
  mount(e) {
    for (let t of this.manager.tooltipViews)
      t.mount && t.mount(e);
    this.mounted = !0;
  }
  positioned(e) {
    for (let t of this.manager.tooltipViews)
      t.positioned && t.positioned(e);
  }
  update(e) {
    this.manager.update(e);
  }
  destroy() {
    var e;
    for (let t of this.manager.tooltipViews)
      (e = t.destroy) === null || e === void 0 || e.call(t);
  }
  passProp(e) {
    let t;
    for (let i of this.manager.tooltipViews) {
      let r = i[e];
      if (r !== void 0) {
        if (t === void 0)
          t = r;
        else if (t !== r)
          return;
      }
    }
    return t;
  }
  get offset() {
    return this.passProp("offset");
  }
  get getCoords() {
    return this.passProp("getCoords");
  }
  get overlap() {
    return this.passProp("overlap");
  }
  get resize() {
    return this.passProp("resize");
  }
}
const vv = /* @__PURE__ */ na.compute([us], (n) => {
  let e = n.facet(us);
  return e.length === 0 ? null : {
    pos: Math.min(...e.map((t) => t.pos)),
    end: Math.max(...e.map((t) => {
      var i;
      return (i = t.end) !== null && i !== void 0 ? i : t.pos;
    })),
    create: Ws.create,
    above: e[0].above,
    arrow: e.some((t) => t.arrow)
  };
});
class yv {
  constructor(e, t, i, r, s) {
    this.view = e, this.source = t, this.field = i, this.setHover = r, this.hoverTime = s, this.hoverTimeout = -1, this.restartTimeout = -1, this.pending = null, this.lastMove = { x: 0, y: 0, target: e.dom, time: 0 }, this.checkHover = this.checkHover.bind(this), e.dom.addEventListener("mouseleave", this.mouseleave = this.mouseleave.bind(this)), e.dom.addEventListener("mousemove", this.mousemove = this.mousemove.bind(this));
  }
  update() {
    this.pending && (this.pending = null, clearTimeout(this.restartTimeout), this.restartTimeout = setTimeout(() => this.startHover(), 20));
  }
  get active() {
    return this.view.state.field(this.field);
  }
  checkHover() {
    if (this.hoverTimeout = -1, this.active.length)
      return;
    let e = Date.now() - this.lastMove.time;
    e < this.hoverTime ? this.hoverTimeout = setTimeout(this.checkHover, this.hoverTime - e) : this.startHover();
  }
  startHover() {
    clearTimeout(this.restartTimeout);
    let { view: e, lastMove: t } = this, i = e.docView.tile.nearest(t.target);
    if (!i)
      return;
    let r, s = 1;
    if (i.isWidget())
      r = i.posAtStart;
    else {
      if (r = e.posAtCoords(t), r == null)
        return;
      let l = e.coordsAtPos(r);
      if (!l || t.y < l.top || t.y > l.bottom || t.x < l.left - e.defaultCharacterWidth || t.x > l.right + e.defaultCharacterWidth)
        return;
      let a = e.bidiSpans(e.state.doc.lineAt(r)).find((u) => u.from <= r && u.to >= r), f = a && a.dir == xe.RTL ? -1 : 1;
      s = t.x < l.left ? -f : f;
    }
    let o = this.source(e, r, s);
    if (o?.then) {
      let l = this.pending = { pos: r };
      o.then((a) => {
        this.pending == l && (this.pending = null, a && !(Array.isArray(a) && !a.length) && e.dispatch({ effects: this.setHover.of(Array.isArray(a) ? a : [a]) }));
      }, (a) => ft(e.state, a, "hover tooltip"));
    } else o && !(Array.isArray(o) && !o.length) && e.dispatch({ effects: this.setHover.of(Array.isArray(o) ? o : [o]) });
  }
  get tooltip() {
    let e = this.view.plugin(ia), t = e ? e.manager.tooltips.findIndex((i) => i.create == Ws.create) : -1;
    return t > -1 ? e.manager.tooltipViews[t] : null;
  }
  mousemove(e) {
    var t, i;
    this.lastMove = { x: e.clientX, y: e.clientY, target: e.target, time: Date.now() }, this.hoverTimeout < 0 && (this.hoverTimeout = setTimeout(this.checkHover, this.hoverTime));
    let { active: r, tooltip: s } = this;
    if (r.length && s && !bv(s.dom, e) || this.pending) {
      let { pos: o } = r[0] || this.pending, l = (i = (t = r[0]) === null || t === void 0 ? void 0 : t.end) !== null && i !== void 0 ? i : o;
      (o == l ? this.view.posAtCoords(this.lastMove) != o : !xv(this.view, o, l, e.clientX, e.clientY)) && (this.view.dispatch({ effects: this.setHover.of([]) }), this.pending = null);
    }
  }
  mouseleave(e) {
    clearTimeout(this.hoverTimeout), this.hoverTimeout = -1;
    let { active: t } = this;
    if (t.length) {
      let { tooltip: i } = this;
      i && i.dom.contains(e.relatedTarget) ? this.watchTooltipLeave(i.dom) : this.view.dispatch({ effects: this.setHover.of([]) });
    }
  }
  watchTooltipLeave(e) {
    let t = (i) => {
      e.removeEventListener("mouseleave", t), this.active.length && !this.view.dom.contains(i.relatedTarget) && this.view.dispatch({ effects: this.setHover.of([]) });
    };
    e.addEventListener("mouseleave", t);
  }
  destroy() {
    clearTimeout(this.hoverTimeout), clearTimeout(this.restartTimeout), this.view.dom.removeEventListener("mouseleave", this.mouseleave), this.view.dom.removeEventListener("mousemove", this.mousemove);
  }
}
const Br = 4;
function bv(n, e) {
  let { left: t, right: i, top: r, bottom: s } = n.getBoundingClientRect(), o;
  if (o = n.querySelector(".cm-tooltip-arrow")) {
    let l = o.getBoundingClientRect();
    r = Math.min(l.top, r), s = Math.max(l.bottom, s);
  }
  return e.clientX >= t - Br && e.clientX <= i + Br && e.clientY >= r - Br && e.clientY <= s + Br;
}
function xv(n, e, t, i, r, s) {
  let o = n.scrollDOM.getBoundingClientRect(), l = n.documentTop + n.documentPadding.top + n.contentHeight;
  if (o.left > i || o.right < i || o.top > r || Math.min(o.bottom, l) < r)
    return !1;
  let a = n.posAtCoords({ x: i, y: r }, !1);
  return a >= e && a <= t;
}
function kv(n, e = {}) {
  let t = ne.define(), i = $e.define({
    create() {
      return [];
    },
    update(r, s) {
      if (r.length && (e.hideOnChange && (s.docChanged || s.selection) ? r = [] : e.hideOn && (r = r.filter((o) => !e.hideOn(s, o))), s.docChanged)) {
        let o = [];
        for (let l of r) {
          let a = s.changes.mapPos(l.pos, -1, Xe.TrackDel);
          if (a != null) {
            let f = Object.assign(/* @__PURE__ */ Object.create(null), l);
            f.pos = a, f.end != null && (f.end = s.changes.mapPos(f.end)), o.push(f);
          }
        }
        r = o;
      }
      for (let o of s.effects)
        o.is(t) && (r = o.value), o.is(wv) && (r = []);
      return r;
    },
    provide: (r) => us.from(r)
  });
  return {
    active: i,
    extension: [
      i,
      De.define((r) => new yv(
        r,
        n,
        i,
        t,
        e.hoverTime || 300
        /* Hover.Time */
      )),
      vv
    ]
  };
}
function Hu(n, e) {
  let t = n.plugin(ia);
  if (!t)
    return null;
  let i = t.manager.tooltips.indexOf(e);
  return i < 0 ? null : t.manager.tooltipViews[i];
}
const wv = /* @__PURE__ */ ne.define(), rc = /* @__PURE__ */ U.define({
  combine(n) {
    let e, t;
    for (let i of n)
      e = e || i.topContainer, t = t || i.bottomContainer;
    return { topContainer: e, bottomContainer: t };
  }
});
function ra(n, e) {
  let t = n.plugin(zu), i = t ? t.specs.indexOf(e) : -1;
  return i > -1 ? t.panels[i] : null;
}
const zu = /* @__PURE__ */ De.fromClass(class {
  constructor(n) {
    this.input = n.state.facet(ji), this.specs = this.input.filter((t) => t), this.panels = this.specs.map((t) => t(n));
    let e = n.state.facet(rc);
    this.top = new Er(n, !0, e.topContainer), this.bottom = new Er(n, !1, e.bottomContainer), this.top.sync(this.panels.filter((t) => t.top)), this.bottom.sync(this.panels.filter((t) => !t.top));
    for (let t of this.panels)
      t.dom.classList.add("cm-panel"), t.mount && t.mount();
  }
  update(n) {
    let e = n.state.facet(rc);
    this.top.container != e.topContainer && (this.top.sync([]), this.top = new Er(n.view, !0, e.topContainer)), this.bottom.container != e.bottomContainer && (this.bottom.sync([]), this.bottom = new Er(n.view, !1, e.bottomContainer)), this.top.syncClasses(), this.bottom.syncClasses();
    let t = n.state.facet(ji);
    if (t != this.input) {
      let i = t.filter((a) => a), r = [], s = [], o = [], l = [];
      for (let a of i) {
        let f = this.specs.indexOf(a), u;
        f < 0 ? (u = a(n.view), l.push(u)) : (u = this.panels[f], u.update && u.update(n)), r.push(u), (u.top ? s : o).push(u);
      }
      this.specs = i, this.panels = r, this.top.sync(s), this.bottom.sync(o);
      for (let a of l)
        a.dom.classList.add("cm-panel"), a.mount && a.mount();
    } else
      for (let i of this.panels)
        i.update && i.update(n);
  }
  destroy() {
    this.top.sync([]), this.bottom.sync([]);
  }
}, {
  provide: (n) => _.scrollMargins.of((e) => {
    let t = e.plugin(n);
    return t && { top: t.top.scrollMargin(), bottom: t.bottom.scrollMargin() };
  })
});
class Er {
  constructor(e, t, i) {
    this.view = e, this.top = t, this.container = i, this.dom = void 0, this.classes = "", this.panels = [], this.syncClasses();
  }
  sync(e) {
    for (let t of this.panels)
      t.destroy && e.indexOf(t) < 0 && t.destroy();
    this.panels = e, this.syncDOM();
  }
  syncDOM() {
    if (this.panels.length == 0) {
      this.dom && (this.dom.remove(), this.dom = void 0);
      return;
    }
    if (!this.dom) {
      this.dom = document.createElement("div"), this.dom.className = this.top ? "cm-panels cm-panels-top" : "cm-panels cm-panels-bottom", this.dom.style[this.top ? "top" : "bottom"] = "0";
      let t = this.container || this.view.dom;
      t.insertBefore(this.dom, this.top ? t.firstChild : null);
    }
    let e = this.dom.firstChild;
    for (let t of this.panels)
      if (t.dom.parentNode == this.dom) {
        for (; e != t.dom; )
          e = sc(e);
        e = e.nextSibling;
      } else
        this.dom.insertBefore(t.dom, e);
    for (; e; )
      e = sc(e);
  }
  scrollMargin() {
    return !this.dom || this.container ? 0 : Math.max(0, this.top ? this.dom.getBoundingClientRect().bottom - Math.max(0, this.view.scrollDOM.getBoundingClientRect().top) : Math.min(innerHeight, this.view.scrollDOM.getBoundingClientRect().bottom) - this.dom.getBoundingClientRect().top);
  }
  syncClasses() {
    if (!(!this.container || this.classes == this.view.themeClasses)) {
      for (let e of this.classes.split(" "))
        e && this.container.classList.remove(e);
      for (let e of (this.classes = this.view.themeClasses).split(" "))
        e && this.container.classList.add(e);
    }
  }
}
function sc(n) {
  let e = n.nextSibling;
  return n.remove(), e;
}
const ji = /* @__PURE__ */ U.define({
  enables: zu
});
function Sv(n, e) {
  let t, i = new Promise((o) => t = o), r = (o) => Cv(o, e, t);
  n.state.field(xo, !1) ? n.dispatch({ effects: $u.of(r) }) : n.dispatch({ effects: ne.appendConfig.of(xo.init(() => [r])) });
  let s = qu.of(r);
  return { close: s, result: i.then((o) => ((n.win.queueMicrotask || ((a) => n.win.setTimeout(a, 10)))(() => {
    n.state.field(xo).indexOf(r) > -1 && n.dispatch({ effects: s });
  }), o)) };
}
const xo = /* @__PURE__ */ $e.define({
  create() {
    return [];
  },
  update(n, e) {
    for (let t of e.effects)
      t.is($u) ? n = [t.value].concat(n) : t.is(qu) && (n = n.filter((i) => i != t.value));
    return n;
  },
  provide: (n) => ji.computeN([n], (e) => e.field(n))
}), $u = /* @__PURE__ */ ne.define(), qu = /* @__PURE__ */ ne.define();
function Cv(n, e, t) {
  let i = e.content ? e.content(n, () => o(null)) : null;
  if (!i) {
    if (i = ye("form"), e.input) {
      let l = ye("input", e.input);
      /^(text|password|number|email|tel|url)$/.test(l.type) && l.classList.add("cm-textfield"), l.name || (l.name = "input"), i.appendChild(ye("label", (e.label || "") + ": ", l));
    } else
      i.appendChild(document.createTextNode(e.label || ""));
    i.appendChild(document.createTextNode(" ")), i.appendChild(ye("button", { class: "cm-button", type: "submit" }, e.submitLabel || "OK"));
  }
  let r = i.nodeName == "FORM" ? [i] : i.querySelectorAll("form");
  for (let l = 0; l < r.length; l++) {
    let a = r[l];
    a.addEventListener("keydown", (f) => {
      f.keyCode == 27 ? (f.preventDefault(), o(null)) : f.keyCode == 13 && (f.preventDefault(), o(a));
    }), a.addEventListener("submit", (f) => {
      f.preventDefault(), o(a);
    });
  }
  let s = ye("div", i, ye("button", {
    onclick: () => o(null),
    "aria-label": n.state.phrase("close"),
    class: "cm-dialog-close",
    type: "button"
  }, ["×"]));
  e.class && (s.className = e.class), s.classList.add("cm-dialog");
  function o(l) {
    s.contains(s.ownerDocument.activeElement) && n.focus(), t(l);
  }
  return {
    dom: s,
    top: e.top,
    mount: () => {
      if (e.focus) {
        let l;
        typeof e.focus == "string" ? l = i.querySelector(e.focus) : l = i.querySelector("input") || i.querySelector("button"), l && "select" in l ? l.select() : l && "focus" in l && l.focus();
      }
    }
  };
}
class ai extends ki {
  /**
  @internal
  */
  compare(e) {
    return this == e || this.constructor == e.constructor && this.eq(e);
  }
  /**
  Compare this marker to another marker of the same type.
  */
  eq(e) {
    return !1;
  }
  /**
  Called if the marker has a `toDOM` method and its representation
  was removed from a gutter.
  */
  destroy(e) {
  }
}
ai.prototype.elementClass = "";
ai.prototype.toDOM = void 0;
ai.prototype.mapMode = Xe.TrackBefore;
ai.prototype.startSide = ai.prototype.endSide = -1;
ai.prototype.point = !0;
const Gr = /* @__PURE__ */ U.define(), Ov = /* @__PURE__ */ U.define(), Av = {
  class: "",
  renderEmptyElements: !1,
  elementStyle: "",
  markers: () => ce.empty,
  lineMarker: () => null,
  widgetMarker: () => null,
  lineMarkerChange: null,
  initialSpacer: null,
  updateSpacer: null,
  domEventHandlers: {},
  side: "before"
}, Un = /* @__PURE__ */ U.define();
function Mv(n) {
  return [Ku(), Un.of({ ...Av, ...n })];
}
const oc = /* @__PURE__ */ U.define({
  combine: (n) => n.some((e) => e)
});
function Ku(n) {
  return [
    Tv
  ];
}
const Tv = /* @__PURE__ */ De.fromClass(class {
  constructor(n) {
    this.view = n, this.domAfter = null, this.prevViewport = n.viewport, this.dom = document.createElement("div"), this.dom.className = "cm-gutters cm-gutters-before", this.dom.setAttribute("aria-hidden", "true"), this.dom.style.minHeight = this.view.contentHeight / this.view.scaleY + "px", this.gutters = n.state.facet(Un).map((e) => new ac(n, e)), this.fixed = !n.state.facet(oc);
    for (let e of this.gutters)
      e.config.side == "after" ? this.getDOMAfter().appendChild(e.dom) : this.dom.appendChild(e.dom);
    this.fixed && (this.dom.style.position = "sticky"), this.syncGutters(!1), n.scrollDOM.insertBefore(this.dom, n.contentDOM);
  }
  getDOMAfter() {
    return this.domAfter || (this.domAfter = document.createElement("div"), this.domAfter.className = "cm-gutters cm-gutters-after", this.domAfter.setAttribute("aria-hidden", "true"), this.domAfter.style.minHeight = this.view.contentHeight / this.view.scaleY + "px", this.domAfter.style.position = this.fixed ? "sticky" : "", this.view.scrollDOM.appendChild(this.domAfter)), this.domAfter;
  }
  update(n) {
    if (this.updateGutters(n)) {
      let e = this.prevViewport, t = n.view.viewport, i = Math.min(e.to, t.to) - Math.max(e.from, t.from);
      this.syncGutters(i < (t.to - t.from) * 0.8);
    }
    if (n.geometryChanged) {
      let e = this.view.contentHeight / this.view.scaleY + "px";
      this.dom.style.minHeight = e, this.domAfter && (this.domAfter.style.minHeight = e);
    }
    this.view.state.facet(oc) != !this.fixed && (this.fixed = !this.fixed, this.dom.style.position = this.fixed ? "sticky" : "", this.domAfter && (this.domAfter.style.position = this.fixed ? "sticky" : "")), this.prevViewport = n.view.viewport;
  }
  syncGutters(n) {
    let e = this.dom.nextSibling;
    n && (this.dom.remove(), this.domAfter && this.domAfter.remove());
    let t = ce.iter(this.view.state.facet(Gr), this.view.viewport.from), i = [], r = this.gutters.map((s) => new Pv(s, this.view.viewport, -this.view.documentPadding.top));
    for (let s of this.view.viewportLineBlocks)
      if (i.length && (i = []), Array.isArray(s.type)) {
        let o = !0;
        for (let l of s.type)
          if (l.type == Ye.Text && o) {
            bl(t, i, l.from);
            for (let a of r)
              a.line(this.view, l, i);
            o = !1;
          } else if (l.widget)
            for (let a of r)
              a.widget(this.view, l);
      } else if (s.type == Ye.Text) {
        bl(t, i, s.from);
        for (let o of r)
          o.line(this.view, s, i);
      } else if (s.widget)
        for (let o of r)
          o.widget(this.view, s);
    for (let s of r)
      s.finish();
    n && (this.view.scrollDOM.insertBefore(this.dom, e), this.domAfter && this.view.scrollDOM.appendChild(this.domAfter));
  }
  updateGutters(n) {
    let e = n.startState.facet(Un), t = n.state.facet(Un), i = n.docChanged || n.heightChanged || n.viewportChanged || !ce.eq(n.startState.facet(Gr), n.state.facet(Gr), n.view.viewport.from, n.view.viewport.to);
    if (e == t)
      for (let r of this.gutters)
        r.update(n) && (i = !0);
    else {
      i = !0;
      let r = [];
      for (let s of t) {
        let o = e.indexOf(s);
        o < 0 ? r.push(new ac(this.view, s)) : (this.gutters[o].update(n), r.push(this.gutters[o]));
      }
      for (let s of this.gutters)
        s.dom.remove(), r.indexOf(s) < 0 && s.destroy();
      for (let s of r)
        s.config.side == "after" ? this.getDOMAfter().appendChild(s.dom) : this.dom.appendChild(s.dom);
      this.gutters = r;
    }
    return i;
  }
  destroy() {
    for (let n of this.gutters)
      n.destroy();
    this.dom.remove(), this.domAfter && this.domAfter.remove();
  }
}, {
  provide: (n) => _.scrollMargins.of((e) => {
    let t = e.plugin(n);
    if (!t || t.gutters.length == 0 || !t.fixed)
      return null;
    let i = t.dom.offsetWidth * e.scaleX, r = t.domAfter ? t.domAfter.offsetWidth * e.scaleX : 0;
    return e.textDirection == xe.LTR ? { left: i, right: r } : { right: i, left: r };
  })
});
function lc(n) {
  return Array.isArray(n) ? n : [n];
}
function bl(n, e, t) {
  for (; n.value && n.from <= t; )
    n.from == t && e.push(n.value), n.next();
}
class Pv {
  constructor(e, t, i) {
    this.gutter = e, this.height = i, this.i = 0, this.cursor = ce.iter(e.markers, t.from);
  }
  addElement(e, t, i) {
    let { gutter: r } = this, s = (t.top - this.height) / e.scaleY, o = t.height / e.scaleY;
    if (this.i == r.elements.length) {
      let l = new _u(e, o, s, i);
      r.elements.push(l), r.dom.appendChild(l.dom);
    } else
      r.elements[this.i].update(e, o, s, i);
    this.height = t.bottom, this.i++;
  }
  line(e, t, i) {
    let r = [];
    bl(this.cursor, r, t.from), i.length && (r = r.concat(i));
    let s = this.gutter.config.lineMarker(e, t, r);
    s && r.unshift(s);
    let o = this.gutter;
    r.length == 0 && !o.config.renderEmptyElements || this.addElement(e, t, r);
  }
  widget(e, t) {
    let i = this.gutter.config.widgetMarker(e, t.widget, t), r = i ? [i] : null;
    for (let s of e.state.facet(Ov)) {
      let o = s(e, t.widget, t);
      o && (r || (r = [])).push(o);
    }
    r && this.addElement(e, t, r);
  }
  finish() {
    let e = this.gutter;
    for (; e.elements.length > this.i; ) {
      let t = e.elements.pop();
      e.dom.removeChild(t.dom), t.destroy();
    }
  }
}
class ac {
  constructor(e, t) {
    this.view = e, this.config = t, this.elements = [], this.spacer = null, this.dom = document.createElement("div"), this.dom.className = "cm-gutter" + (this.config.class ? " " + this.config.class : "");
    for (let i in t.domEventHandlers)
      this.dom.addEventListener(i, (r) => {
        let s = r.target, o;
        if (s != this.dom && this.dom.contains(s)) {
          for (; s.parentNode != this.dom; )
            s = s.parentNode;
          let a = s.getBoundingClientRect();
          o = (a.top + a.bottom) / 2;
        } else
          o = r.clientY;
        let l = e.lineBlockAtHeight(o - e.documentTop);
        t.domEventHandlers[i](e, l, r) && r.preventDefault();
      });
    this.markers = lc(t.markers(e)), t.initialSpacer && (this.spacer = new _u(e, 0, 0, [t.initialSpacer(e)]), this.dom.appendChild(this.spacer.dom), this.spacer.dom.style.cssText += "visibility: hidden; pointer-events: none");
  }
  update(e) {
    let t = this.markers;
    if (this.markers = lc(this.config.markers(e.view)), this.spacer && this.config.updateSpacer) {
      let r = this.config.updateSpacer(this.spacer.markers[0], e);
      r != this.spacer.markers[0] && this.spacer.update(e.view, 0, 0, [r]);
    }
    let i = e.view.viewport;
    return !ce.eq(this.markers, t, i.from, i.to) || (this.config.lineMarkerChange ? this.config.lineMarkerChange(e) : !1);
  }
  destroy() {
    for (let e of this.elements)
      e.destroy();
  }
}
class _u {
  constructor(e, t, i, r) {
    this.height = -1, this.above = 0, this.markers = [], this.dom = document.createElement("div"), this.dom.className = "cm-gutterElement", this.update(e, t, i, r);
  }
  update(e, t, i, r) {
    this.height != t && (this.height = t, this.dom.style.height = t + "px"), this.above != i && (this.dom.style.marginTop = (this.above = i) ? i + "px" : ""), Rv(this.markers, r) || this.setMarkers(e, r);
  }
  setMarkers(e, t) {
    let i = "cm-gutterElement", r = this.dom.firstChild;
    for (let s = 0, o = 0; ; ) {
      let l = o, a = s < t.length ? t[s++] : null, f = !1;
      if (a) {
        let u = a.elementClass;
        u && (i += " " + u);
        for (let g = o; g < this.markers.length; g++)
          if (this.markers[g].compare(a)) {
            l = g, f = !0;
            break;
          }
      } else
        l = this.markers.length;
      for (; o < l; ) {
        let u = this.markers[o++];
        if (u.toDOM) {
          u.destroy(r);
          let g = r.nextSibling;
          r.remove(), r = g;
        }
      }
      if (!a)
        break;
      a.toDOM && (f ? r = r.nextSibling : this.dom.insertBefore(a.toDOM(e), r)), f && o++;
    }
    this.dom.className = i, this.markers = t;
  }
  destroy() {
    this.setMarkers(null, []);
  }
}
function Rv(n, e) {
  if (n.length != e.length)
    return !1;
  for (let t = 0; t < n.length; t++)
    if (!n[t].compare(e[t]))
      return !1;
  return !0;
}
const Lv = /* @__PURE__ */ U.define(), Dv = /* @__PURE__ */ U.define(), on = /* @__PURE__ */ U.define({
  combine(n) {
    return ti(n, { formatNumber: String, domEventHandlers: {} }, {
      domEventHandlers(e, t) {
        let i = Object.assign({}, e);
        for (let r in t) {
          let s = i[r], o = t[r];
          i[r] = s ? (l, a, f) => s(l, a, f) || o(l, a, f) : o;
        }
        return i;
      }
    });
  }
});
class ko extends ai {
  constructor(e) {
    super(), this.number = e;
  }
  eq(e) {
    return this.number == e.number;
  }
  toDOM() {
    return document.createTextNode(this.number);
  }
}
function wo(n, e) {
  return n.state.facet(on).formatNumber(e, n.state);
}
const Bv = /* @__PURE__ */ Un.compute([on], (n) => ({
  class: "cm-lineNumbers",
  renderEmptyElements: !1,
  markers(e) {
    return e.state.facet(Lv);
  },
  lineMarker(e, t, i) {
    return i.some((r) => r.toDOM) ? null : new ko(wo(e, e.state.doc.lineAt(t.from).number));
  },
  widgetMarker: (e, t, i) => {
    for (let r of e.state.facet(Dv)) {
      let s = r(e, t, i);
      if (s)
        return s;
    }
    return null;
  },
  lineMarkerChange: (e) => e.startState.facet(on) != e.state.facet(on),
  initialSpacer(e) {
    return new ko(wo(e, hc(e.state.doc.lines)));
  },
  updateSpacer(e, t) {
    let i = wo(t.view, hc(t.view.state.doc.lines));
    return i == e.number ? e : new ko(i);
  },
  domEventHandlers: n.facet(on).domEventHandlers,
  side: "before"
}));
function Ev(n = {}) {
  return [
    on.of(n),
    Ku(),
    Bv
  ];
}
function hc(n) {
  let e = 9;
  for (; e < n; )
    e = e * 10 + 9;
  return e;
}
const Iv = /* @__PURE__ */ new class extends ai {
  constructor() {
    super(...arguments), this.elementClass = "cm-activeLineGutter";
  }
}(), Nv = /* @__PURE__ */ Gr.compute(["selection"], (n) => {
  let e = [], t = -1;
  for (let i of n.selection.ranges) {
    let r = n.doc.lineAt(i.head).from;
    r > t && (t = r, e.push(Iv.range(r)));
  }
  return ce.of(e);
});
function Fv() {
  return Nv;
}
const ju = 1024;
let Wv = 0;
class At {
  constructor(e, t) {
    this.from = e, this.to = t;
  }
}
class oe {
  /**
  Create a new node prop type.
  */
  constructor(e = {}) {
    this.id = Wv++, this.perNode = !!e.perNode, this.deserialize = e.deserialize || (() => {
      throw new Error("This node type doesn't define a deserialize function");
    }), this.combine = e.combine || null;
  }
  /**
  This is meant to be used with
  [`NodeSet.extend`](#common.NodeSet.extend) or
  [`LRParser.configure`](#lr.ParserConfig.props) to compute
  prop values for each node type in the set. Takes a [match
  object](#common.NodeType^match) or function that returns undefined
  if the node type doesn't get this prop, and the prop's value if
  it does.
  */
  add(e) {
    if (this.perNode)
      throw new RangeError("Can't add per-node props to node types");
    return typeof e != "function" && (e = ot.match(e)), (t) => {
      let i = e(t);
      return i === void 0 ? null : [this, i];
    };
  }
}
oe.closedBy = new oe({ deserialize: (n) => n.split(" ") });
oe.openedBy = new oe({ deserialize: (n) => n.split(" ") });
oe.group = new oe({ deserialize: (n) => n.split(" ") });
oe.isolate = new oe({ deserialize: (n) => {
  if (n && n != "rtl" && n != "ltr" && n != "auto")
    throw new RangeError("Invalid value for isolate: " + n);
  return n || "auto";
} });
oe.contextHash = new oe({ perNode: !0 });
oe.lookAhead = new oe({ perNode: !0 });
oe.mounted = new oe({ perNode: !0 });
class dn {
  constructor(e, t, i, r = !1) {
    this.tree = e, this.overlay = t, this.parser = i, this.bracketed = r;
  }
  /**
  @internal
  */
  static get(e) {
    return e && e.props && e.props[oe.mounted.id];
  }
}
const Qv = /* @__PURE__ */ Object.create(null);
class ot {
  /**
  @internal
  */
  constructor(e, t, i, r = 0) {
    this.name = e, this.props = t, this.id = i, this.flags = r;
  }
  /**
  Define a node type.
  */
  static define(e) {
    let t = e.props && e.props.length ? /* @__PURE__ */ Object.create(null) : Qv, i = (e.top ? 1 : 0) | (e.skipped ? 2 : 0) | (e.error ? 4 : 0) | (e.name == null ? 8 : 0), r = new ot(e.name || "", t, e.id, i);
    if (e.props) {
      for (let s of e.props)
        if (Array.isArray(s) || (s = s(r)), s) {
          if (s[0].perNode)
            throw new RangeError("Can't store a per-node prop on a node type");
          t[s[0].id] = s[1];
        }
    }
    return r;
  }
  /**
  Retrieves a node prop for this type. Will return `undefined` if
  the prop isn't present on this node.
  */
  prop(e) {
    return this.props[e.id];
  }
  /**
  True when this is the top node of a grammar.
  */
  get isTop() {
    return (this.flags & 1) > 0;
  }
  /**
  True when this node is produced by a skip rule.
  */
  get isSkipped() {
    return (this.flags & 2) > 0;
  }
  /**
  Indicates whether this is an error node.
  */
  get isError() {
    return (this.flags & 4) > 0;
  }
  /**
  When true, this node type doesn't correspond to a user-declared
  named node, for example because it is used to cache repetition.
  */
  get isAnonymous() {
    return (this.flags & 8) > 0;
  }
  /**
  Returns true when this node's name or one of its
  [groups](#common.NodeProp^group) matches the given string.
  */
  is(e) {
    if (typeof e == "string") {
      if (this.name == e)
        return !0;
      let t = this.prop(oe.group);
      return t ? t.indexOf(e) > -1 : !1;
    }
    return this.id == e;
  }
  /**
  Create a function from node types to arbitrary values by
  specifying an object whose property names are node or
  [group](#common.NodeProp^group) names. Often useful with
  [`NodeProp.add`](#common.NodeProp.add). You can put multiple
  names, separated by spaces, in a single property name to map
  multiple node names to a single value.
  */
  static match(e) {
    let t = /* @__PURE__ */ Object.create(null);
    for (let i in e)
      for (let r of i.split(" "))
        t[r] = e[i];
    return (i) => {
      for (let r = i.prop(oe.group), s = -1; s < (r ? r.length : 0); s++) {
        let o = t[s < 0 ? i.name : r[s]];
        if (o)
          return o;
      }
    };
  }
}
ot.none = new ot(
  "",
  /* @__PURE__ */ Object.create(null),
  0,
  8
  /* NodeFlag.Anonymous */
);
class sa {
  /**
  Create a set with the given types. The `id` property of each
  type should correspond to its position within the array.
  */
  constructor(e) {
    this.types = e;
    for (let t = 0; t < e.length; t++)
      if (e[t].id != t)
        throw new RangeError("Node type ids should correspond to array positions when creating a node set");
  }
  /**
  Create a copy of this set with some node properties added. The
  arguments to this method can be created with
  [`NodeProp.add`](#common.NodeProp.add).
  */
  extend(...e) {
    let t = [];
    for (let i of this.types) {
      let r = null;
      for (let s of e) {
        let o = s(i);
        if (o) {
          r || (r = Object.assign({}, i.props));
          let l = o[1], a = o[0];
          a.combine && a.id in r && (l = a.combine(r[a.id], l)), r[a.id] = l;
        }
      }
      t.push(r ? new ot(i.name, r, i.id, i.flags) : i);
    }
    return new sa(t);
  }
}
const Ir = /* @__PURE__ */ new WeakMap(), cc = /* @__PURE__ */ new WeakMap();
var be;
(function(n) {
  n[n.ExcludeBuffers = 1] = "ExcludeBuffers", n[n.IncludeAnonymous = 2] = "IncludeAnonymous", n[n.IgnoreMounts = 4] = "IgnoreMounts", n[n.IgnoreOverlays = 8] = "IgnoreOverlays", n[n.EnterBracketed = 16] = "EnterBracketed";
})(be || (be = {}));
class Te {
  /**
  Construct a new tree. See also [`Tree.build`](#common.Tree^build).
  */
  constructor(e, t, i, r, s) {
    if (this.type = e, this.children = t, this.positions = i, this.length = r, this.props = null, s && s.length) {
      this.props = /* @__PURE__ */ Object.create(null);
      for (let [o, l] of s)
        this.props[typeof o == "number" ? o : o.id] = l;
    }
  }
  /**
  @internal
  */
  toString() {
    let e = dn.get(this);
    if (e && !e.overlay)
      return e.tree.toString();
    let t = "";
    for (let i of this.children) {
      let r = i.toString();
      r && (t && (t += ","), t += r);
    }
    return this.type.name ? (/\W/.test(this.type.name) && !this.type.isError ? JSON.stringify(this.type.name) : this.type.name) + (t.length ? "(" + t + ")" : "") : t;
  }
  /**
  Get a [tree cursor](#common.TreeCursor) positioned at the top of
  the tree. Mode can be used to [control](#common.IterMode) which
  nodes the cursor visits.
  */
  cursor(e = 0) {
    return new ds(this.topNode, e);
  }
  /**
  Get a [tree cursor](#common.TreeCursor) pointing into this tree
  at the given position and side (see
  [`moveTo`](#common.TreeCursor.moveTo).
  */
  cursorAt(e, t = 0, i = 0) {
    let r = Ir.get(this) || this.topNode, s = new ds(r);
    return s.moveTo(e, t), Ir.set(this, s._tree), s;
  }
  /**
  Get a [syntax node](#common.SyntaxNode) object for the top of the
  tree.
  */
  get topNode() {
    return new dt(this, 0, 0, null);
  }
  /**
  Get the [syntax node](#common.SyntaxNode) at the given position.
  If `side` is -1, this will move into nodes that end at the
  position. If 1, it'll move into nodes that start at the
  position. With 0, it'll only enter nodes that cover the position
  from both sides.
  
  Note that this will not enter
  [overlays](#common.MountedTree.overlay), and you often want
  [`resolveInner`](#common.Tree.resolveInner) instead.
  */
  resolve(e, t = 0) {
    let i = er(Ir.get(this) || this.topNode, e, t, !1);
    return Ir.set(this, i), i;
  }
  /**
  Like [`resolve`](#common.Tree.resolve), but will enter
  [overlaid](#common.MountedTree.overlay) nodes, producing a syntax node
  pointing into the innermost overlaid tree at the given position
  (with parent links going through all parent structure, including
  the host trees).
  */
  resolveInner(e, t = 0) {
    let i = er(cc.get(this) || this.topNode, e, t, !0);
    return cc.set(this, i), i;
  }
  /**
  In some situations, it can be useful to iterate through all
  nodes around a position, including those in overlays that don't
  directly cover the position. This method gives you an iterator
  that will produce all nodes, from small to big, around the given
  position.
  */
  resolveStack(e, t = 0) {
    return zv(this, e, t);
  }
  /**
  Iterate over the tree and its children, calling `enter` for any
  node that touches the `from`/`to` region (if given) before
  running over such a node's children, and `leave` (if given) when
  leaving the node. When `enter` returns `false`, that node will
  not have its children iterated over (or `leave` called).
  */
  iterate(e) {
    let { enter: t, leave: i, from: r = 0, to: s = this.length } = e, o = e.mode || 0, l = (o & be.IncludeAnonymous) > 0;
    for (let a = this.cursor(o | be.IncludeAnonymous); ; ) {
      let f = !1;
      if (a.from <= s && a.to >= r && (!l && a.type.isAnonymous || t(a) !== !1)) {
        if (a.firstChild())
          continue;
        f = !0;
      }
      for (; f && i && (l || !a.type.isAnonymous) && i(a), !a.nextSibling(); ) {
        if (!a.parent())
          return;
        f = !0;
      }
    }
  }
  /**
  Get the value of the given [node prop](#common.NodeProp) for this
  node. Works with both per-node and per-type props.
  */
  prop(e) {
    return e.perNode ? this.props ? this.props[e.id] : void 0 : this.type.prop(e);
  }
  /**
  Returns the node's [per-node props](#common.NodeProp.perNode) in a
  format that can be passed to the [`Tree`](#common.Tree)
  constructor.
  */
  get propValues() {
    let e = [];
    if (this.props)
      for (let t in this.props)
        e.push([+t, this.props[t]]);
    return e;
  }
  /**
  Balance the direct children of this tree, producing a copy of
  which may have children grouped into subtrees with type
  [`NodeType.none`](#common.NodeType^none).
  */
  balance(e = {}) {
    return this.children.length <= 8 ? this : aa(ot.none, this.children, this.positions, 0, this.children.length, 0, this.length, (t, i, r) => new Te(this.type, t, i, r, this.propValues), e.makeTree || ((t, i, r) => new Te(ot.none, t, i, r)));
  }
  /**
  Build a tree from a postfix-ordered buffer of node information,
  or a cursor over such a buffer.
  */
  static build(e) {
    return $v(e);
  }
}
Te.empty = new Te(ot.none, [], [], 0);
class oa {
  constructor(e, t) {
    this.buffer = e, this.index = t;
  }
  get id() {
    return this.buffer[this.index - 4];
  }
  get start() {
    return this.buffer[this.index - 3];
  }
  get end() {
    return this.buffer[this.index - 2];
  }
  get size() {
    return this.buffer[this.index - 1];
  }
  get pos() {
    return this.index;
  }
  next() {
    this.index -= 4;
  }
  fork() {
    return new oa(this.buffer, this.index);
  }
}
class Oi {
  /**
  Create a tree buffer.
  */
  constructor(e, t, i) {
    this.buffer = e, this.length = t, this.set = i;
  }
  /**
  @internal
  */
  get type() {
    return ot.none;
  }
  /**
  @internal
  */
  toString() {
    let e = [];
    for (let t = 0; t < this.buffer.length; )
      e.push(this.childString(t)), t = this.buffer[t + 3];
    return e.join(",");
  }
  /**
  @internal
  */
  childString(e) {
    let t = this.buffer[e], i = this.buffer[e + 3], r = this.set.types[t], s = r.name;
    if (/\W/.test(s) && !r.isError && (s = JSON.stringify(s)), e += 4, i == e)
      return s;
    let o = [];
    for (; e < i; )
      o.push(this.childString(e)), e = this.buffer[e + 3];
    return s + "(" + o.join(",") + ")";
  }
  /**
  @internal
  */
  findChild(e, t, i, r, s) {
    let { buffer: o } = this, l = -1;
    for (let a = e; a != t && !(Uu(s, r, o[a + 1], o[a + 2]) && (l = a, i > 0)); a = o[a + 3])
      ;
    return l;
  }
  /**
  @internal
  */
  slice(e, t, i) {
    let r = this.buffer, s = new Uint16Array(t - e), o = 0;
    for (let l = e, a = 0; l < t; ) {
      s[a++] = r[l++], s[a++] = r[l++] - i;
      let f = s[a++] = r[l++] - i;
      s[a++] = r[l++] - e, o = Math.max(o, f);
    }
    return new Oi(s, o, this.set);
  }
}
function Uu(n, e, t, i) {
  switch (n) {
    case -2:
      return t < e;
    case -1:
      return i >= e && t < e;
    case 0:
      return t < e && i > e;
    case 1:
      return t <= e && i > e;
    case 2:
      return i > e;
    case 4:
      return !0;
  }
}
function er(n, e, t, i) {
  for (var r; n.from == n.to || (t < 1 ? n.from >= e : n.from > e) || (t > -1 ? n.to <= e : n.to < e); ) {
    let o = !i && n instanceof dt && n.index < 0 ? null : n.parent;
    if (!o)
      return n;
    n = o;
  }
  let s = i ? 0 : be.IgnoreOverlays;
  if (i)
    for (let o = n, l = o.parent; l; o = l, l = o.parent)
      o instanceof dt && o.index < 0 && ((r = l.enter(e, t, s)) === null || r === void 0 ? void 0 : r.from) != o.from && (n = l);
  for (; ; ) {
    let o = n.enter(e, t, s);
    if (!o)
      return n;
    n = o;
  }
}
class Xu {
  cursor(e = 0) {
    return new ds(this, e);
  }
  getChild(e, t = null, i = null) {
    let r = fc(this, e, t, i);
    return r.length ? r[0] : null;
  }
  getChildren(e, t = null, i = null) {
    return fc(this, e, t, i);
  }
  resolve(e, t = 0) {
    return er(this, e, t, !1);
  }
  resolveInner(e, t = 0) {
    return er(this, e, t, !0);
  }
  matchContext(e) {
    return xl(this.parent, e);
  }
  enterUnfinishedNodesBefore(e) {
    let t = this.childBefore(e), i = this;
    for (; t; ) {
      let r = t.lastChild;
      if (!r || r.to != t.to)
        break;
      r.type.isError && r.from == r.to ? (i = t, t = r.prevSibling) : t = r;
    }
    return i;
  }
  get node() {
    return this;
  }
  get next() {
    return this.parent;
  }
}
class dt extends Xu {
  constructor(e, t, i, r) {
    super(), this._tree = e, this.from = t, this.index = i, this._parent = r;
  }
  get type() {
    return this._tree.type;
  }
  get name() {
    return this._tree.type.name;
  }
  get to() {
    return this.from + this._tree.length;
  }
  nextChild(e, t, i, r, s = 0) {
    var o;
    for (let l = this; ; ) {
      for (let { children: a, positions: f } = l._tree, u = t > 0 ? a.length : -1; e != u; e += t) {
        let g = a[e], y = f[e] + l.from;
        if (!(!(s & be.EnterBracketed && g instanceof Te && ((o = dn.get(g)) === null || o === void 0 ? void 0 : o.overlay) === null && (y >= i || y + g.length <= i)) && !Uu(r, i, y, y + g.length))) {
          if (g instanceof Oi) {
            if (s & be.ExcludeBuffers)
              continue;
            let b = g.findChild(0, g.buffer.length, t, i - y, r);
            if (b > -1)
              return new yi(new Vv(l, g, e, y), null, b);
          } else if (s & be.IncludeAnonymous || !g.type.isAnonymous || la(g)) {
            let b;
            if (!(s & be.IgnoreMounts) && (b = dn.get(g)) && !b.overlay)
              return new dt(b.tree, y, e, l);
            let k = new dt(g, y, e, l);
            return s & be.IncludeAnonymous || !k.type.isAnonymous ? k : k.nextChild(t < 0 ? g.children.length - 1 : 0, t, i, r, s);
          }
        }
      }
      if (s & be.IncludeAnonymous || !l.type.isAnonymous || (l.index >= 0 ? e = l.index + t : e = t < 0 ? -1 : l._parent._tree.children.length, l = l._parent, !l))
        return null;
    }
  }
  get firstChild() {
    return this.nextChild(
      0,
      1,
      0,
      4
      /* Side.DontCare */
    );
  }
  get lastChild() {
    return this.nextChild(
      this._tree.children.length - 1,
      -1,
      0,
      4
      /* Side.DontCare */
    );
  }
  childAfter(e) {
    return this.nextChild(
      0,
      1,
      e,
      2
      /* Side.After */
    );
  }
  childBefore(e) {
    return this.nextChild(
      this._tree.children.length - 1,
      -1,
      e,
      -2
      /* Side.Before */
    );
  }
  prop(e) {
    return this._tree.prop(e);
  }
  enter(e, t, i = 0) {
    let r;
    if (!(i & be.IgnoreOverlays) && (r = dn.get(this._tree)) && r.overlay) {
      let s = e - this.from, o = i & be.EnterBracketed && r.bracketed;
      for (let { from: l, to: a } of r.overlay)
        if ((t > 0 || o ? l <= s : l < s) && (t < 0 || o ? a >= s : a > s))
          return new dt(r.tree, r.overlay[0].from + this.from, -1, this);
    }
    return this.nextChild(0, 1, e, t, i);
  }
  nextSignificantParent() {
    let e = this;
    for (; e.type.isAnonymous && e._parent; )
      e = e._parent;
    return e;
  }
  get parent() {
    return this._parent ? this._parent.nextSignificantParent() : null;
  }
  get nextSibling() {
    return this._parent && this.index >= 0 ? this._parent.nextChild(
      this.index + 1,
      1,
      0,
      4
      /* Side.DontCare */
    ) : null;
  }
  get prevSibling() {
    return this._parent && this.index >= 0 ? this._parent.nextChild(
      this.index - 1,
      -1,
      0,
      4
      /* Side.DontCare */
    ) : null;
  }
  get tree() {
    return this._tree;
  }
  toTree() {
    return this._tree;
  }
  /**
  @internal
  */
  toString() {
    return this._tree.toString();
  }
}
function fc(n, e, t, i) {
  let r = n.cursor(), s = [];
  if (!r.firstChild())
    return s;
  if (t != null) {
    for (let o = !1; !o; )
      if (o = r.type.is(t), !r.nextSibling())
        return s;
  }
  for (; ; ) {
    if (i != null && r.type.is(i))
      return s;
    if (r.type.is(e) && s.push(r.node), !r.nextSibling())
      return i == null ? s : [];
  }
}
function xl(n, e, t = e.length - 1) {
  for (let i = n; t >= 0; i = i.parent) {
    if (!i)
      return !1;
    if (!i.type.isAnonymous) {
      if (e[t] && e[t] != i.name)
        return !1;
      t--;
    }
  }
  return !0;
}
class Vv {
  constructor(e, t, i, r) {
    this.parent = e, this.buffer = t, this.index = i, this.start = r;
  }
}
class yi extends Xu {
  get name() {
    return this.type.name;
  }
  get from() {
    return this.context.start + this.context.buffer.buffer[this.index + 1];
  }
  get to() {
    return this.context.start + this.context.buffer.buffer[this.index + 2];
  }
  constructor(e, t, i) {
    super(), this.context = e, this._parent = t, this.index = i, this.type = e.buffer.set.types[e.buffer.buffer[i]];
  }
  child(e, t, i) {
    let { buffer: r } = this.context, s = r.findChild(this.index + 4, r.buffer[this.index + 3], e, t - this.context.start, i);
    return s < 0 ? null : new yi(this.context, this, s);
  }
  get firstChild() {
    return this.child(
      1,
      0,
      4
      /* Side.DontCare */
    );
  }
  get lastChild() {
    return this.child(
      -1,
      0,
      4
      /* Side.DontCare */
    );
  }
  childAfter(e) {
    return this.child(
      1,
      e,
      2
      /* Side.After */
    );
  }
  childBefore(e) {
    return this.child(
      -1,
      e,
      -2
      /* Side.Before */
    );
  }
  prop(e) {
    return this.type.prop(e);
  }
  enter(e, t, i = 0) {
    if (i & be.ExcludeBuffers)
      return null;
    let { buffer: r } = this.context, s = r.findChild(this.index + 4, r.buffer[this.index + 3], t > 0 ? 1 : -1, e - this.context.start, t);
    return s < 0 ? null : new yi(this.context, this, s);
  }
  get parent() {
    return this._parent || this.context.parent.nextSignificantParent();
  }
  externalSibling(e) {
    return this._parent ? null : this.context.parent.nextChild(
      this.context.index + e,
      e,
      0,
      4
      /* Side.DontCare */
    );
  }
  get nextSibling() {
    let { buffer: e } = this.context, t = e.buffer[this.index + 3];
    return t < (this._parent ? e.buffer[this._parent.index + 3] : e.buffer.length) ? new yi(this.context, this._parent, t) : this.externalSibling(1);
  }
  get prevSibling() {
    let { buffer: e } = this.context, t = this._parent ? this._parent.index + 4 : 0;
    return this.index == t ? this.externalSibling(-1) : new yi(this.context, this._parent, e.findChild(
      t,
      this.index,
      -1,
      0,
      4
      /* Side.DontCare */
    ));
  }
  get tree() {
    return null;
  }
  toTree() {
    let e = [], t = [], { buffer: i } = this.context, r = this.index + 4, s = i.buffer[this.index + 3];
    if (s > r) {
      let o = i.buffer[this.index + 1];
      e.push(i.slice(r, s, o)), t.push(0);
    }
    return new Te(this.type, e, t, this.to - this.from);
  }
  /**
  @internal
  */
  toString() {
    return this.context.buffer.childString(this.index);
  }
}
function Yu(n) {
  if (!n.length)
    return null;
  let e = 0, t = n[0];
  for (let s = 1; s < n.length; s++) {
    let o = n[s];
    (o.from > t.from || o.to < t.to) && (t = o, e = s);
  }
  let i = t instanceof dt && t.index < 0 ? null : t.parent, r = n.slice();
  return i ? r[e] = i : r.splice(e, 1), new Hv(r, t);
}
class Hv {
  constructor(e, t) {
    this.heads = e, this.node = t;
  }
  get next() {
    return Yu(this.heads);
  }
}
function zv(n, e, t) {
  let i = n.resolveInner(e, t), r = null;
  for (let s = i instanceof dt ? i : i.context.parent; s; s = s.parent)
    if (s.index < 0) {
      let o = s.parent;
      (r || (r = [i])).push(o.resolve(e, t)), s = o;
    } else {
      let o = dn.get(s.tree);
      if (o && o.overlay && o.overlay[0].from <= e && o.overlay[o.overlay.length - 1].to >= e) {
        let l = new dt(o.tree, o.overlay[0].from + s.from, -1, s);
        (r || (r = [i])).push(er(l, e, t, !1));
      }
    }
  return r ? Yu(r) : i;
}
class ds {
  /**
  Shorthand for `.type.name`.
  */
  get name() {
    return this.type.name;
  }
  /**
  @internal
  */
  constructor(e, t = 0) {
    if (this.buffer = null, this.stack = [], this.index = 0, this.bufferNode = null, this.mode = t & ~be.EnterBracketed, e instanceof dt)
      this.yieldNode(e);
    else {
      this._tree = e.context.parent, this.buffer = e.context;
      for (let i = e._parent; i; i = i._parent)
        this.stack.unshift(i.index);
      this.bufferNode = e, this.yieldBuf(e.index);
    }
  }
  yieldNode(e) {
    return e ? (this._tree = e, this.type = e.type, this.from = e.from, this.to = e.to, !0) : !1;
  }
  yieldBuf(e, t) {
    this.index = e;
    let { start: i, buffer: r } = this.buffer;
    return this.type = t || r.set.types[r.buffer[e]], this.from = i + r.buffer[e + 1], this.to = i + r.buffer[e + 2], !0;
  }
  /**
  @internal
  */
  yield(e) {
    return e ? e instanceof dt ? (this.buffer = null, this.yieldNode(e)) : (this.buffer = e.context, this.yieldBuf(e.index, e.type)) : !1;
  }
  /**
  @internal
  */
  toString() {
    return this.buffer ? this.buffer.buffer.childString(this.index) : this._tree.toString();
  }
  /**
  @internal
  */
  enterChild(e, t, i) {
    if (!this.buffer)
      return this.yield(this._tree.nextChild(e < 0 ? this._tree._tree.children.length - 1 : 0, e, t, i, this.mode));
    let { buffer: r } = this.buffer, s = r.findChild(this.index + 4, r.buffer[this.index + 3], e, t - this.buffer.start, i);
    return s < 0 ? !1 : (this.stack.push(this.index), this.yieldBuf(s));
  }
  /**
  Move the cursor to this node's first child. When this returns
  false, the node has no child, and the cursor has not been moved.
  */
  firstChild() {
    return this.enterChild(
      1,
      0,
      4
      /* Side.DontCare */
    );
  }
  /**
  Move the cursor to this node's last child.
  */
  lastChild() {
    return this.enterChild(
      -1,
      0,
      4
      /* Side.DontCare */
    );
  }
  /**
  Move the cursor to the first child that ends after `pos`.
  */
  childAfter(e) {
    return this.enterChild(
      1,
      e,
      2
      /* Side.After */
    );
  }
  /**
  Move to the last child that starts before `pos`.
  */
  childBefore(e) {
    return this.enterChild(
      -1,
      e,
      -2
      /* Side.Before */
    );
  }
  /**
  Move the cursor to the child around `pos`. If side is -1 the
  child may end at that position, when 1 it may start there. This
  will also enter [overlaid](#common.MountedTree.overlay)
  [mounted](#common.NodeProp^mounted) trees unless `overlays` is
  set to false.
  */
  enter(e, t, i = this.mode) {
    return this.buffer ? i & be.ExcludeBuffers ? !1 : this.enterChild(1, e, t) : this.yield(this._tree.enter(e, t, i));
  }
  /**
  Move to the node's parent node, if this isn't the top node.
  */
  parent() {
    if (!this.buffer)
      return this.yieldNode(this.mode & be.IncludeAnonymous ? this._tree._parent : this._tree.parent);
    if (this.stack.length)
      return this.yieldBuf(this.stack.pop());
    let e = this.mode & be.IncludeAnonymous ? this.buffer.parent : this.buffer.parent.nextSignificantParent();
    return this.buffer = null, this.yieldNode(e);
  }
  /**
  @internal
  */
  sibling(e) {
    if (!this.buffer)
      return this._tree._parent ? this.yield(this._tree.index < 0 ? null : this._tree._parent.nextChild(this._tree.index + e, e, 0, 4, this.mode)) : !1;
    let { buffer: t } = this.buffer, i = this.stack.length - 1;
    if (e < 0) {
      let r = i < 0 ? 0 : this.stack[i] + 4;
      if (this.index != r)
        return this.yieldBuf(t.findChild(
          r,
          this.index,
          -1,
          0,
          4
          /* Side.DontCare */
        ));
    } else {
      let r = t.buffer[this.index + 3];
      if (r < (i < 0 ? t.buffer.length : t.buffer[this.stack[i] + 3]))
        return this.yieldBuf(r);
    }
    return i < 0 ? this.yield(this.buffer.parent.nextChild(this.buffer.index + e, e, 0, 4, this.mode)) : !1;
  }
  /**
  Move to this node's next sibling, if any.
  */
  nextSibling() {
    return this.sibling(1);
  }
  /**
  Move to this node's previous sibling, if any.
  */
  prevSibling() {
    return this.sibling(-1);
  }
  atLastNode(e) {
    let t, i, { buffer: r } = this;
    if (r) {
      if (e > 0) {
        if (this.index < r.buffer.buffer.length)
          return !1;
      } else
        for (let s = 0; s < this.index; s++)
          if (r.buffer.buffer[s + 3] < this.index)
            return !1;
      ({ index: t, parent: i } = r);
    } else
      ({ index: t, _parent: i } = this._tree);
    for (; i; { index: t, _parent: i } = i)
      if (t > -1)
        for (let s = t + e, o = e < 0 ? -1 : i._tree.children.length; s != o; s += e) {
          let l = i._tree.children[s];
          if (this.mode & be.IncludeAnonymous || l instanceof Oi || !l.type.isAnonymous || la(l))
            return !1;
        }
    return !0;
  }
  move(e, t) {
    if (t && this.enterChild(
      e,
      0,
      4
      /* Side.DontCare */
    ))
      return !0;
    for (; ; ) {
      if (this.sibling(e))
        return !0;
      if (this.atLastNode(e) || !this.parent())
        return !1;
    }
  }
  /**
  Move to the next node in a
  [pre-order](https://en.wikipedia.org/wiki/Tree_traversal#Pre-order,_NLR)
  traversal, going from a node to its first child or, if the
  current node is empty or `enter` is false, its next sibling or
  the next sibling of the first parent node that has one.
  */
  next(e = !0) {
    return this.move(1, e);
  }
  /**
  Move to the next node in a last-to-first pre-order traversal. A
  node is followed by its last child or, if it has none, its
  previous sibling or the previous sibling of the first parent
  node that has one.
  */
  prev(e = !0) {
    return this.move(-1, e);
  }
  /**
  Move the cursor to the innermost node that covers `pos`. If
  `side` is -1, it will enter nodes that end at `pos`. If it is 1,
  it will enter nodes that start at `pos`.
  */
  moveTo(e, t = 0) {
    for (; (this.from == this.to || (t < 1 ? this.from >= e : this.from > e) || (t > -1 ? this.to <= e : this.to < e)) && this.parent(); )
      ;
    for (; this.enterChild(1, e, t); )
      ;
    return this;
  }
  /**
  Get a [syntax node](#common.SyntaxNode) at the cursor's current
  position.
  */
  get node() {
    if (!this.buffer)
      return this._tree;
    let e = this.bufferNode, t = null, i = 0;
    if (e && e.context == this.buffer)
      e: for (let r = this.index, s = this.stack.length; s >= 0; ) {
        for (let o = e; o; o = o._parent)
          if (o.index == r) {
            if (r == this.index)
              return o;
            t = o, i = s + 1;
            break e;
          }
        r = this.stack[--s];
      }
    for (let r = i; r < this.stack.length; r++)
      t = new yi(this.buffer, t, this.stack[r]);
    return this.bufferNode = new yi(this.buffer, t, this.index);
  }
  /**
  Get the [tree](#common.Tree) that represents the current node, if
  any. Will return null when the node is in a [tree
  buffer](#common.TreeBuffer).
  */
  get tree() {
    return this.buffer ? null : this._tree._tree;
  }
  /**
  Iterate over the current node and all its descendants, calling
  `enter` when entering a node and `leave`, if given, when leaving
  one. When `enter` returns `false`, any children of that node are
  skipped, and `leave` isn't called for it.
  */
  iterate(e, t) {
    for (let i = 0; ; ) {
      let r = !1;
      if (this.type.isAnonymous || e(this) !== !1) {
        if (this.firstChild()) {
          i++;
          continue;
        }
        this.type.isAnonymous || (r = !0);
      }
      for (; ; ) {
        if (r && t && t(this), r = this.type.isAnonymous, !i)
          return;
        if (this.nextSibling())
          break;
        this.parent(), i--, r = !0;
      }
    }
  }
  /**
  Test whether the current node matches a given context—a sequence
  of direct parent node names. Empty strings in the context array
  are treated as wildcards.
  */
  matchContext(e) {
    if (!this.buffer)
      return xl(this.node.parent, e);
    let { buffer: t } = this.buffer, { types: i } = t.set;
    for (let r = e.length - 1, s = this.stack.length - 1; r >= 0; s--) {
      if (s < 0)
        return xl(this._tree, e, r);
      let o = i[t.buffer[this.stack[s]]];
      if (!o.isAnonymous) {
        if (e[r] && e[r] != o.name)
          return !1;
        r--;
      }
    }
    return !0;
  }
}
function la(n) {
  return n.children.some((e) => e instanceof Oi || !e.type.isAnonymous || la(e));
}
function $v(n) {
  var e;
  let { buffer: t, nodeSet: i, maxBufferLength: r = ju, reused: s = [], minRepeatType: o = i.types.length } = n, l = Array.isArray(t) ? new oa(t, t.length) : t, a = i.types, f = 0, u = 0;
  function g(F, H, Q, Z, le, he) {
    let { id: ee, start: Y, end: ie, size: fe } = l, me = u, qe = f;
    if (fe < 0)
      if (l.next(), fe == -1) {
        let tt = s[ee];
        Q.push(tt), Z.push(Y - F);
        return;
      } else if (fe == -3) {
        f = ee;
        return;
      } else if (fe == -4) {
        u = ee;
        return;
      } else
        throw new RangeError(`Unrecognized record size: ${fe}`);
    let Be = a[ee], q, Ee, Ge = Y - F;
    if (ie - Y <= r && (Ee = M(l.pos - H, le))) {
      let tt = new Uint16Array(Ee.size - Ee.skip), we = l.pos - Ee.size, Ke = tt.length;
      for (; l.pos > we; )
        Ke = R(Ee.start, tt, Ke);
      q = new Oi(tt, ie - Ee.start, i), Ge = Ee.start - F;
    } else {
      let tt = l.pos - fe;
      l.next();
      let we = [], Ke = [], bt = ee >= o ? ee : -1, ve = 0, Yi = ie;
      for (; l.pos > tt; )
        bt >= 0 && l.id == bt && l.size >= 0 ? (l.end <= Yi - r && (k(we, Ke, Y, ve, l.end, Yi, bt, me, qe), ve = we.length, Yi = l.end), l.next()) : he > 2500 ? y(Y, tt, we, Ke) : g(Y, tt, we, Ke, bt, he + 1);
      if (bt >= 0 && ve > 0 && ve < we.length && k(we, Ke, Y, ve, Y, Yi, bt, me, qe), we.reverse(), Ke.reverse(), bt > -1 && ve > 0) {
        let fi = b(Be, qe);
        q = aa(Be, we, Ke, 0, we.length, 0, ie - Y, fi, fi);
      } else
        q = C(Be, we, Ke, ie - Y, me - ie, qe);
    }
    Q.push(q), Z.push(Ge);
  }
  function y(F, H, Q, Z) {
    let le = [], he = 0, ee = -1;
    for (; l.pos > H; ) {
      let { id: Y, start: ie, end: fe, size: me } = l;
      if (me > 4)
        l.next();
      else {
        if (ee > -1 && ie < ee)
          break;
        ee < 0 && (ee = fe - r), le.push(Y, ie, fe), he++, l.next();
      }
    }
    if (he) {
      let Y = new Uint16Array(he * 4), ie = le[le.length - 2];
      for (let fe = le.length - 3, me = 0; fe >= 0; fe -= 3)
        Y[me++] = le[fe], Y[me++] = le[fe + 1] - ie, Y[me++] = le[fe + 2] - ie, Y[me++] = me;
      Q.push(new Oi(Y, le[2] - ie, i)), Z.push(ie - F);
    }
  }
  function b(F, H) {
    return (Q, Z, le) => {
      let he = 0, ee = Q.length - 1, Y, ie;
      if (ee >= 0 && (Y = Q[ee]) instanceof Te) {
        if (!ee && Y.type == F && Y.length == le)
          return Y;
        (ie = Y.prop(oe.lookAhead)) && (he = Z[ee] + Y.length + ie);
      }
      return C(F, Q, Z, le, he, H);
    };
  }
  function k(F, H, Q, Z, le, he, ee, Y, ie) {
    let fe = [], me = [];
    for (; F.length > Z; )
      fe.push(F.pop()), me.push(H.pop() + Q - le);
    F.push(C(i.types[ee], fe, me, he - le, Y - he, ie)), H.push(le - Q);
  }
  function C(F, H, Q, Z, le, he, ee) {
    if (he) {
      let Y = [oe.contextHash, he];
      ee = ee ? [Y].concat(ee) : [Y];
    }
    if (le > 25) {
      let Y = [oe.lookAhead, le];
      ee = ee ? [Y].concat(ee) : [Y];
    }
    return new Te(F, H, Q, Z, ee);
  }
  function M(F, H) {
    let Q = l.fork(), Z = 0, le = 0, he = 0, ee = Q.end - r, Y = { size: 0, start: 0, skip: 0 };
    e: for (let ie = Q.pos - F; Q.pos > ie; ) {
      let fe = Q.size;
      if (Q.id == H && fe >= 0) {
        Y.size = Z, Y.start = le, Y.skip = he, he += 4, Z += 4, Q.next();
        continue;
      }
      let me = Q.pos - fe;
      if (fe < 0 || me < ie || Q.start < ee)
        break;
      let qe = Q.id >= o ? 4 : 0, Be = Q.start;
      for (Q.next(); Q.pos > me; ) {
        if (Q.size < 0)
          if (Q.size == -3 || Q.size == -4)
            qe += 4;
          else
            break e;
        else Q.id >= o && (qe += 4);
        Q.next();
      }
      le = Be, Z += fe, he += qe;
    }
    return (H < 0 || Z == F) && (Y.size = Z, Y.start = le, Y.skip = he), Y.size > 4 ? Y : void 0;
  }
  function R(F, H, Q) {
    let { id: Z, start: le, end: he, size: ee } = l;
    if (l.next(), ee >= 0 && Z < o) {
      let Y = Q;
      if (ee > 4) {
        let ie = l.pos - (ee - 4);
        for (; l.pos > ie; )
          Q = R(F, H, Q);
      }
      H[--Q] = Y, H[--Q] = he - F, H[--Q] = le - F, H[--Q] = Z;
    } else ee == -3 ? f = Z : ee == -4 && (u = Z);
    return Q;
  }
  let I = [], N = [];
  for (; l.pos > 0; )
    g(n.start || 0, n.bufferStart || 0, I, N, -1, 0);
  let z = (e = n.length) !== null && e !== void 0 ? e : I.length ? N[0] + I[0].length : 0;
  return new Te(a[n.topID], I.reverse(), N.reverse(), z);
}
const uc = /* @__PURE__ */ new WeakMap();
function Zr(n, e) {
  if (!n.isAnonymous || e instanceof Oi || e.type != n)
    return 1;
  let t = uc.get(e);
  if (t == null) {
    t = 1;
    for (let i of e.children) {
      if (i.type != n || !(i instanceof Te)) {
        t = 1;
        break;
      }
      t += Zr(n, i);
    }
    uc.set(e, t);
  }
  return t;
}
function aa(n, e, t, i, r, s, o, l, a) {
  let f = 0;
  for (let k = i; k < r; k++)
    f += Zr(n, e[k]);
  let u = Math.ceil(
    f * 1.5 / 8
    /* Balance.BranchFactor */
  ), g = [], y = [];
  function b(k, C, M, R, I) {
    for (let N = M; N < R; ) {
      let z = N, F = C[N], H = Zr(n, k[N]);
      for (N++; N < R; N++) {
        let Q = Zr(n, k[N]);
        if (H + Q >= u)
          break;
        H += Q;
      }
      if (N == z + 1) {
        if (H > u) {
          let Q = k[z];
          b(Q.children, Q.positions, 0, Q.children.length, C[z] + I);
          continue;
        }
        g.push(k[z]);
      } else {
        let Q = C[N - 1] + k[N - 1].length - F;
        g.push(aa(n, k, C, z, N, F, Q, null, a));
      }
      y.push(F + I - s);
    }
  }
  return b(e, t, i, r, 0), (l || a)(g, y, o);
}
class oi {
  /**
  Construct a tree fragment. You'll usually want to use
  [`addTree`](#common.TreeFragment^addTree) and
  [`applyChanges`](#common.TreeFragment^applyChanges) instead of
  calling this directly.
  */
  constructor(e, t, i, r, s = !1, o = !1) {
    this.from = e, this.to = t, this.tree = i, this.offset = r, this.open = (s ? 1 : 0) | (o ? 2 : 0);
  }
  /**
  Whether the start of the fragment represents the start of a
  parse, or the end of a change. (In the second case, it may not
  be safe to reuse some nodes at the start, depending on the
  parsing algorithm.)
  */
  get openStart() {
    return (this.open & 1) > 0;
  }
  /**
  Whether the end of the fragment represents the end of a
  full-document parse, or the start of a change.
  */
  get openEnd() {
    return (this.open & 2) > 0;
  }
  /**
  Create a set of fragments from a freshly parsed tree, or update
  an existing set of fragments by replacing the ones that overlap
  with a tree with content from the new tree. When `partial` is
  true, the parse is treated as incomplete, and the resulting
  fragment has [`openEnd`](#common.TreeFragment.openEnd) set to
  true.
  */
  static addTree(e, t = [], i = !1) {
    let r = [new oi(0, e.length, e, 0, !1, i)];
    for (let s of t)
      s.to > e.length && r.push(s);
    return r;
  }
  /**
  Apply a set of edits to an array of fragments, removing or
  splitting fragments as necessary to remove edited ranges, and
  adjusting offsets for fragments that moved.
  */
  static applyChanges(e, t, i = 128) {
    if (!t.length)
      return e;
    let r = [], s = 1, o = e.length ? e[0] : null;
    for (let l = 0, a = 0, f = 0; ; l++) {
      let u = l < t.length ? t[l] : null, g = u ? u.fromA : 1e9;
      if (g - a >= i)
        for (; o && o.from < g; ) {
          let y = o;
          if (a >= y.from || g <= y.to || f) {
            let b = Math.max(y.from, a) - f, k = Math.min(y.to, g) - f;
            y = b >= k ? null : new oi(b, k, y.tree, y.offset + f, l > 0, !!u);
          }
          if (y && r.push(y), o.to > g)
            break;
          o = s < e.length ? e[s++] : null;
        }
      if (!u)
        break;
      a = u.toA, f = u.toA - u.toB;
    }
    return r;
  }
}
class Gu {
  /**
  Start a parse, returning a [partial parse](#common.PartialParse)
  object. [`fragments`](#common.TreeFragment) can be passed in to
  make the parse incremental.
  
  By default, the entire input is parsed. You can pass `ranges`,
  which should be a sorted array of non-empty, non-overlapping
  ranges, to parse only those ranges. The tree returned in that
  case will start at `ranges[0].from`.
  */
  startParse(e, t, i) {
    return typeof e == "string" && (e = new qv(e)), i = i ? i.length ? i.map((r) => new At(r.from, r.to)) : [new At(0, 0)] : [new At(0, e.length)], this.createParse(e, t || [], i);
  }
  /**
  Run a full parse, returning the resulting tree.
  */
  parse(e, t, i) {
    let r = this.startParse(e, t, i);
    for (; ; ) {
      let s = r.advance();
      if (s)
        return s;
    }
  }
}
class qv {
  constructor(e) {
    this.string = e;
  }
  get length() {
    return this.string.length;
  }
  chunk(e) {
    return this.string.slice(e);
  }
  get lineChunks() {
    return !1;
  }
  read(e, t) {
    return this.string.slice(e, t);
  }
}
function Kv(n) {
  return (e, t, i, r) => new jv(e, n, t, i, r);
}
class dc {
  constructor(e, t, i, r, s, o) {
    this.parser = e, this.parse = t, this.overlay = i, this.bracketed = r, this.target = s, this.from = o;
  }
}
function pc(n) {
  if (!n.length || n.some((e) => e.from >= e.to))
    throw new RangeError("Invalid inner parse ranges given: " + JSON.stringify(n));
}
class _v {
  constructor(e, t, i, r, s, o, l, a) {
    this.parser = e, this.predicate = t, this.mounts = i, this.index = r, this.start = s, this.bracketed = o, this.target = l, this.prev = a, this.depth = 0, this.ranges = [];
  }
}
const kl = new oe({ perNode: !0 });
class jv {
  constructor(e, t, i, r, s) {
    this.nest = t, this.input = i, this.fragments = r, this.ranges = s, this.inner = [], this.innerDone = 0, this.baseTree = null, this.stoppedAt = null, this.baseParse = e;
  }
  advance() {
    if (this.baseParse) {
      let i = this.baseParse.advance();
      if (!i)
        return null;
      if (this.baseParse = null, this.baseTree = i, this.startInner(), this.stoppedAt != null)
        for (let r of this.inner)
          r.parse.stopAt(this.stoppedAt);
    }
    if (this.innerDone == this.inner.length) {
      let i = this.baseTree;
      return this.stoppedAt != null && (i = new Te(i.type, i.children, i.positions, i.length, i.propValues.concat([[kl, this.stoppedAt]]))), i;
    }
    let e = this.inner[this.innerDone], t = e.parse.advance();
    if (t) {
      this.innerDone++;
      let i = Object.assign(/* @__PURE__ */ Object.create(null), e.target.props);
      i[oe.mounted.id] = new dn(t, e.overlay, e.parser, e.bracketed), e.target.props = i;
    }
    return null;
  }
  get parsedPos() {
    if (this.baseParse)
      return 0;
    let e = this.input.length;
    for (let t = this.innerDone; t < this.inner.length; t++)
      this.inner[t].from < e && (e = Math.min(e, this.inner[t].parse.parsedPos));
    return e;
  }
  stopAt(e) {
    if (this.stoppedAt = e, this.baseParse)
      this.baseParse.stopAt(e);
    else
      for (let t = this.innerDone; t < this.inner.length; t++)
        this.inner[t].parse.stopAt(e);
  }
  startInner() {
    let e = new Yv(this.fragments), t = null, i = null, r = new ds(new dt(this.baseTree, this.ranges[0].from, 0, null), be.IncludeAnonymous | be.IgnoreMounts);
    e: for (let s, o; ; ) {
      let l = !0, a;
      if (this.stoppedAt != null && r.from >= this.stoppedAt)
        l = !1;
      else if (e.hasNode(r)) {
        if (t) {
          let f = t.mounts.find((u) => u.frag.from <= r.from && u.frag.to >= r.to && u.mount.overlay);
          if (f)
            for (let u of f.mount.overlay) {
              let g = u.from + f.pos, y = u.to + f.pos;
              g >= r.from && y <= r.to && !t.ranges.some((b) => b.from < y && b.to > g) && t.ranges.push({ from: g, to: y });
            }
        }
        l = !1;
      } else if (i && (o = Uv(i.ranges, r.from, r.to)))
        l = o != 2;
      else if (!r.type.isAnonymous && (s = this.nest(r, this.input)) && (r.from < r.to || !s.overlay)) {
        r.tree || (Xv(r), t && t.depth++, i && i.depth++);
        let f = e.findMounts(r.from, s.parser);
        if (typeof s.overlay == "function")
          t = new _v(s.parser, s.overlay, f, this.inner.length, r.from, !!s.bracketed, r.tree, t);
        else {
          let u = vc(this.ranges, s.overlay || (r.from < r.to ? [new At(r.from, r.to)] : []));
          u.length && pc(u), (u.length || !s.overlay) && this.inner.push(new dc(s.parser, u.length ? s.parser.startParse(this.input, yc(f, u), u) : s.parser.startParse(""), s.overlay ? s.overlay.map((g) => new At(g.from - r.from, g.to - r.from)) : null, !!s.bracketed, r.tree, u.length ? u[0].from : r.from)), s.overlay ? u.length && (i = { ranges: u, depth: 0, prev: i }) : l = !1;
        }
      } else if (t && (a = t.predicate(r)) && (a === !0 && (a = new At(r.from, r.to)), a.from < a.to)) {
        let f = t.ranges.length - 1;
        f >= 0 && t.ranges[f].to == a.from ? t.ranges[f] = { from: t.ranges[f].from, to: a.to } : t.ranges.push(a);
      }
      if (l && r.firstChild())
        t && t.depth++, i && i.depth++;
      else
        for (; !r.nextSibling(); ) {
          if (!r.parent())
            break e;
          if (t && !--t.depth) {
            let f = vc(this.ranges, t.ranges);
            f.length && (pc(f), this.inner.splice(t.index, 0, new dc(t.parser, t.parser.startParse(this.input, yc(t.mounts, f), f), t.ranges.map((u) => new At(u.from - t.start, u.to - t.start)), t.bracketed, t.target, f[0].from))), t = t.prev;
          }
          i && !--i.depth && (i = i.prev);
        }
    }
  }
}
function Uv(n, e, t) {
  for (let i of n) {
    if (i.from >= t)
      break;
    if (i.to > e)
      return i.from <= e && i.to >= t ? 2 : 1;
  }
  return 0;
}
function gc(n, e, t, i, r, s) {
  if (e < t) {
    let o = n.buffer[e + 1];
    i.push(n.slice(e, t, o)), r.push(o - s);
  }
}
function Xv(n) {
  let { node: e } = n, t = [], i = e.context.buffer;
  do
    t.push(n.index), n.parent();
  while (!n.tree);
  let r = n.tree, s = r.children.indexOf(i), o = r.children[s], l = o.buffer, a = [s];
  function f(u, g, y, b, k, C) {
    let M = t[C], R = [], I = [];
    gc(o, u, M, R, I, b);
    let N = l[M + 1], z = l[M + 2];
    a.push(R.length);
    let F = C ? f(M + 4, l[M + 3], o.set.types[l[M]], N, z - N, C - 1) : e.toTree();
    return R.push(F), I.push(N - b), gc(o, l[M + 3], g, R, I, b), new Te(y, R, I, k);
  }
  r.children[s] = f(0, l.length, ot.none, 0, o.length, t.length - 1);
  for (let u of a) {
    let g = n.tree.children[u], y = n.tree.positions[u];
    n.yield(new dt(g, y + n.from, u, n._tree));
  }
}
class mc {
  constructor(e, t) {
    this.offset = t, this.done = !1, this.cursor = e.cursor(be.IncludeAnonymous | be.IgnoreMounts);
  }
  // Move to the first node (in pre-order) that starts at or after `pos`.
  moveTo(e) {
    let { cursor: t } = this, i = e - this.offset;
    for (; !this.done && t.from < i; )
      t.to >= e && t.enter(i, 1, be.IgnoreOverlays | be.ExcludeBuffers) || t.next(!1) || (this.done = !0);
  }
  hasNode(e) {
    if (this.moveTo(e.from), !this.done && this.cursor.from + this.offset == e.from && this.cursor.tree)
      for (let t = this.cursor.tree; ; ) {
        if (t == e.tree)
          return !0;
        if (t.children.length && t.positions[0] == 0 && t.children[0] instanceof Te)
          t = t.children[0];
        else
          break;
      }
    return !1;
  }
}
let Yv = class {
  constructor(e) {
    var t;
    if (this.fragments = e, this.curTo = 0, this.fragI = 0, e.length) {
      let i = this.curFrag = e[0];
      this.curTo = (t = i.tree.prop(kl)) !== null && t !== void 0 ? t : i.to, this.inner = new mc(i.tree, -i.offset);
    } else
      this.curFrag = this.inner = null;
  }
  hasNode(e) {
    for (; this.curFrag && e.from >= this.curTo; )
      this.nextFrag();
    return this.curFrag && this.curFrag.from <= e.from && this.curTo >= e.to && this.inner.hasNode(e);
  }
  nextFrag() {
    var e;
    if (this.fragI++, this.fragI == this.fragments.length)
      this.curFrag = this.inner = null;
    else {
      let t = this.curFrag = this.fragments[this.fragI];
      this.curTo = (e = t.tree.prop(kl)) !== null && e !== void 0 ? e : t.to, this.inner = new mc(t.tree, -t.offset);
    }
  }
  findMounts(e, t) {
    var i;
    let r = [];
    if (this.inner) {
      this.inner.cursor.moveTo(e, 1);
      for (let s = this.inner.cursor.node; s; s = s.parent) {
        let o = (i = s.tree) === null || i === void 0 ? void 0 : i.prop(oe.mounted);
        if (o && o.parser == t)
          for (let l = this.fragI; l < this.fragments.length; l++) {
            let a = this.fragments[l];
            if (a.from >= s.to)
              break;
            a.tree == this.curFrag.tree && r.push({
              frag: a,
              pos: s.from - a.offset,
              mount: o
            });
          }
      }
    }
    return r;
  }
};
function vc(n, e) {
  let t = null, i = e;
  for (let r = 1, s = 0; r < n.length; r++) {
    let o = n[r - 1].to, l = n[r].from;
    for (; s < i.length; s++) {
      let a = i[s];
      if (a.from >= l)
        break;
      a.to <= o || (t || (i = t = e.slice()), a.from < o ? (t[s] = new At(a.from, o), a.to > l && t.splice(s + 1, 0, new At(l, a.to))) : a.to > l ? t[s--] = new At(l, a.to) : t.splice(s--, 1));
    }
  }
  return i;
}
function Gv(n, e, t, i) {
  let r = 0, s = 0, o = !1, l = !1, a = -1e9, f = [];
  for (; ; ) {
    let u = r == n.length ? 1e9 : o ? n[r].to : n[r].from, g = s == e.length ? 1e9 : l ? e[s].to : e[s].from;
    if (o != l) {
      let y = Math.max(a, t), b = Math.min(u, g, i);
      y < b && f.push(new At(y, b));
    }
    if (a = Math.min(u, g), a == 1e9)
      break;
    u == a && (o ? (o = !1, r++) : o = !0), g == a && (l ? (l = !1, s++) : l = !0);
  }
  return f;
}
function yc(n, e) {
  let t = [];
  for (let { pos: i, mount: r, frag: s } of n) {
    let o = i + (r.overlay ? r.overlay[0].from : 0), l = o + r.tree.length, a = Math.max(s.from, o), f = Math.min(s.to, l);
    if (r.overlay) {
      let u = r.overlay.map((y) => new At(y.from + i, y.to + i)), g = Gv(e, u, a, f);
      for (let y = 0, b = a; ; y++) {
        let k = y == g.length, C = k ? f : g[y].from;
        if (C > b && t.push(new oi(b, C, r.tree, -o, s.from >= b || s.openStart, s.to <= C || s.openEnd)), k)
          break;
        b = g[y].to;
      }
    } else
      t.push(new oi(a, f, r.tree, -o, s.from >= o || s.openStart, s.to <= l || s.openEnd));
  }
  return t;
}
let Zv = 0;
class Ct {
  /**
  @internal
  */
  constructor(e, t, i, r) {
    this.name = e, this.set = t, this.base = i, this.modified = r, this.id = Zv++;
  }
  toString() {
    let { name: e } = this;
    for (let t of this.modified)
      t.name && (e = `${t.name}(${e})`);
    return e;
  }
  static define(e, t) {
    let i = typeof e == "string" ? e : "?";
    if (e instanceof Ct && (t = e), t?.base)
      throw new Error("Can not derive from a modified tag");
    let r = new Ct(i, [], null, []);
    if (r.set.push(r), t)
      for (let s of t.set)
        r.set.push(s);
    return r;
  }
  /**
  Define a tag _modifier_, which is a function that, given a tag,
  will return a tag that is a subtag of the original. Applying the
  same modifier to a twice tag will return the same value (`m1(t1)
  == m1(t1)`) and applying multiple modifiers will, regardless or
  order, produce the same tag (`m1(m2(t1)) == m2(m1(t1))`).
  
  When multiple modifiers are applied to a given base tag, each
  smaller set of modifiers is registered as a parent, so that for
  example `m1(m2(m3(t1)))` is a subtype of `m1(m2(t1))`,
  `m1(m3(t1)`, and so on.
  */
  static defineModifier(e) {
    let t = new ps(e);
    return (i) => i.modified.indexOf(t) > -1 ? i : ps.get(i.base || i, i.modified.concat(t).sort((r, s) => r.id - s.id));
  }
}
let Jv = 0;
class ps {
  constructor(e) {
    this.name = e, this.instances = [], this.id = Jv++;
  }
  static get(e, t) {
    if (!t.length)
      return e;
    let i = t[0].instances.find((l) => l.base == e && ey(t, l.modified));
    if (i)
      return i;
    let r = [], s = new Ct(e.name, r, e, t);
    for (let l of t)
      l.instances.push(s);
    let o = ty(t);
    for (let l of e.set)
      if (!l.modified.length)
        for (let a of o)
          r.push(ps.get(l, a));
    return s;
  }
}
function ey(n, e) {
  return n.length == e.length && n.every((t, i) => t == e[i]);
}
function ty(n) {
  let e = [[]];
  for (let t = 0; t < n.length; t++)
    for (let i = 0, r = e.length; i < r; i++)
      e.push(e[i].concat(n[t]));
  return e.sort((t, i) => i.length - t.length);
}
function Qs(n) {
  let e = /* @__PURE__ */ Object.create(null);
  for (let t in n) {
    let i = n[t];
    Array.isArray(i) || (i = [i]);
    for (let r of t.split(" "))
      if (r) {
        let s = [], o = 2, l = r;
        for (let g = 0; ; ) {
          if (l == "..." && g > 0 && g + 3 == r.length) {
            o = 1;
            break;
          }
          let y = /^"(?:[^"\\]|\\.)*?"|[^\/!]+/.exec(l);
          if (!y)
            throw new RangeError("Invalid path: " + r);
          if (s.push(y[0] == "*" ? "" : y[0][0] == '"' ? JSON.parse(y[0]) : y[0]), g += y[0].length, g == r.length)
            break;
          let b = r[g++];
          if (g == r.length && b == "!") {
            o = 0;
            break;
          }
          if (b != "/")
            throw new RangeError("Invalid path: " + r);
          l = r.slice(g);
        }
        let a = s.length - 1, f = s[a];
        if (!f)
          throw new RangeError("Invalid path: " + r);
        let u = new tr(i, o, a > 0 ? s.slice(0, a) : null);
        e[f] = u.sort(e[f]);
      }
  }
  return Zu.add(e);
}
const Zu = new oe({
  combine(n, e) {
    let t, i, r;
    for (; n || e; ) {
      if (!n || e && n.depth >= e.depth ? (r = e, e = e.next) : (r = n, n = n.next), t && t.mode == r.mode && !r.context && !t.context)
        continue;
      let s = new tr(r.tags, r.mode, r.context);
      t ? t.next = s : i = s, t = s;
    }
    return i;
  }
});
class tr {
  constructor(e, t, i, r) {
    this.tags = e, this.mode = t, this.context = i, this.next = r;
  }
  get opaque() {
    return this.mode == 0;
  }
  get inherit() {
    return this.mode == 1;
  }
  sort(e) {
    return !e || e.depth < this.depth ? (this.next = e, this) : (e.next = this.sort(e.next), e);
  }
  get depth() {
    return this.context ? this.context.length : 0;
  }
}
tr.empty = new tr([], 2, null);
function Ju(n, e) {
  let t = /* @__PURE__ */ Object.create(null);
  for (let s of n)
    if (!Array.isArray(s.tag))
      t[s.tag.id] = s.class;
    else
      for (let o of s.tag)
        t[o.id] = s.class;
  let { scope: i, all: r = null } = e || {};
  return {
    style: (s) => {
      let o = r;
      for (let l of s)
        for (let a of l.set) {
          let f = t[a.id];
          if (f) {
            o = o ? o + " " + f : f;
            break;
          }
        }
      return o;
    },
    scope: i
  };
}
function iy(n, e) {
  let t = null;
  for (let i of n) {
    let r = i.style(e);
    r && (t = t ? t + " " + r : r);
  }
  return t;
}
function ny(n, e, t, i = 0, r = n.length) {
  let s = new ry(i, Array.isArray(e) ? e : [e], t);
  s.highlightRange(n.cursor(), i, r, "", s.highlighters), s.flush(r);
}
class ry {
  constructor(e, t, i) {
    this.at = e, this.highlighters = t, this.span = i, this.class = "";
  }
  startSpan(e, t) {
    t != this.class && (this.flush(e), e > this.at && (this.at = e), this.class = t);
  }
  flush(e) {
    e > this.at && this.class && this.span(this.at, e, this.class);
  }
  highlightRange(e, t, i, r, s) {
    let { type: o, from: l, to: a } = e;
    if (l >= i || a <= t)
      return;
    o.isTop && (s = this.highlighters.filter((b) => !b.scope || b.scope(o)));
    let f = r, u = sy(e) || tr.empty, g = iy(s, u.tags);
    if (g && (f && (f += " "), f += g, u.mode == 1 && (r += (r ? " " : "") + g)), this.startSpan(Math.max(t, l), f), u.opaque)
      return;
    let y = e.tree && e.tree.prop(oe.mounted);
    if (y && y.overlay) {
      let b = e.node.enter(y.overlay[0].from + l, 1), k = this.highlighters.filter((M) => !M.scope || M.scope(y.tree.type)), C = e.firstChild();
      for (let M = 0, R = l; ; M++) {
        let I = M < y.overlay.length ? y.overlay[M] : null, N = I ? I.from + l : a, z = Math.max(t, R), F = Math.min(i, N);
        if (z < F && C)
          for (; e.from < F && (this.highlightRange(e, z, F, r, s), this.startSpan(Math.min(F, e.to), f), !(e.to >= N || !e.nextSibling())); )
            ;
        if (!I || N > i)
          break;
        R = I.to + l, R > t && (this.highlightRange(b.cursor(), Math.max(t, I.from + l), Math.min(i, R), "", k), this.startSpan(Math.min(i, R), f));
      }
      C && e.parent();
    } else if (e.firstChild()) {
      y && (r = "");
      do
        if (!(e.to <= t)) {
          if (e.from >= i)
            break;
          this.highlightRange(e, t, i, r, s), this.startSpan(Math.min(i, e.to), f);
        }
      while (e.nextSibling());
      e.parent();
    }
  }
}
function sy(n) {
  let e = n.type.prop(Zu);
  for (; e && e.context && !n.matchContext(e.context); )
    e = e.next;
  return e || null;
}
const $ = Ct.define, Nr = $(), pi = $(), bc = $(pi), xc = $(pi), gi = $(), Fr = $(gi), So = $(gi), jt = $(), Di = $(jt), Kt = $(), _t = $(), wl = $(), In = $(wl), Wr = $(), P = {
  /**
  A comment.
  */
  comment: Nr,
  /**
  A line [comment](#highlight.tags.comment).
  */
  lineComment: $(Nr),
  /**
  A block [comment](#highlight.tags.comment).
  */
  blockComment: $(Nr),
  /**
  A documentation [comment](#highlight.tags.comment).
  */
  docComment: $(Nr),
  /**
  Any kind of identifier.
  */
  name: pi,
  /**
  The [name](#highlight.tags.name) of a variable.
  */
  variableName: $(pi),
  /**
  A type [name](#highlight.tags.name).
  */
  typeName: bc,
  /**
  A tag name (subtag of [`typeName`](#highlight.tags.typeName)).
  */
  tagName: $(bc),
  /**
  A property or field [name](#highlight.tags.name).
  */
  propertyName: xc,
  /**
  An attribute name (subtag of [`propertyName`](#highlight.tags.propertyName)).
  */
  attributeName: $(xc),
  /**
  The [name](#highlight.tags.name) of a class.
  */
  className: $(pi),
  /**
  A label [name](#highlight.tags.name).
  */
  labelName: $(pi),
  /**
  A namespace [name](#highlight.tags.name).
  */
  namespace: $(pi),
  /**
  The [name](#highlight.tags.name) of a macro.
  */
  macroName: $(pi),
  /**
  A literal value.
  */
  literal: gi,
  /**
  A string [literal](#highlight.tags.literal).
  */
  string: Fr,
  /**
  A documentation [string](#highlight.tags.string).
  */
  docString: $(Fr),
  /**
  A character literal (subtag of [string](#highlight.tags.string)).
  */
  character: $(Fr),
  /**
  An attribute value (subtag of [string](#highlight.tags.string)).
  */
  attributeValue: $(Fr),
  /**
  A number [literal](#highlight.tags.literal).
  */
  number: So,
  /**
  An integer [number](#highlight.tags.number) literal.
  */
  integer: $(So),
  /**
  A floating-point [number](#highlight.tags.number) literal.
  */
  float: $(So),
  /**
  A boolean [literal](#highlight.tags.literal).
  */
  bool: $(gi),
  /**
  Regular expression [literal](#highlight.tags.literal).
  */
  regexp: $(gi),
  /**
  An escape [literal](#highlight.tags.literal), for example a
  backslash escape in a string.
  */
  escape: $(gi),
  /**
  A color [literal](#highlight.tags.literal).
  */
  color: $(gi),
  /**
  A URL [literal](#highlight.tags.literal).
  */
  url: $(gi),
  /**
  A language keyword.
  */
  keyword: Kt,
  /**
  The [keyword](#highlight.tags.keyword) for the self or this
  object.
  */
  self: $(Kt),
  /**
  The [keyword](#highlight.tags.keyword) for null.
  */
  null: $(Kt),
  /**
  A [keyword](#highlight.tags.keyword) denoting some atomic value.
  */
  atom: $(Kt),
  /**
  A [keyword](#highlight.tags.keyword) that represents a unit.
  */
  unit: $(Kt),
  /**
  A modifier [keyword](#highlight.tags.keyword).
  */
  modifier: $(Kt),
  /**
  A [keyword](#highlight.tags.keyword) that acts as an operator.
  */
  operatorKeyword: $(Kt),
  /**
  A control-flow related [keyword](#highlight.tags.keyword).
  */
  controlKeyword: $(Kt),
  /**
  A [keyword](#highlight.tags.keyword) that defines something.
  */
  definitionKeyword: $(Kt),
  /**
  A [keyword](#highlight.tags.keyword) related to defining or
  interfacing with modules.
  */
  moduleKeyword: $(Kt),
  /**
  An operator.
  */
  operator: _t,
  /**
  An [operator](#highlight.tags.operator) that dereferences something.
  */
  derefOperator: $(_t),
  /**
  Arithmetic-related [operator](#highlight.tags.operator).
  */
  arithmeticOperator: $(_t),
  /**
  Logical [operator](#highlight.tags.operator).
  */
  logicOperator: $(_t),
  /**
  Bit [operator](#highlight.tags.operator).
  */
  bitwiseOperator: $(_t),
  /**
  Comparison [operator](#highlight.tags.operator).
  */
  compareOperator: $(_t),
  /**
  [Operator](#highlight.tags.operator) that updates its operand.
  */
  updateOperator: $(_t),
  /**
  [Operator](#highlight.tags.operator) that defines something.
  */
  definitionOperator: $(_t),
  /**
  Type-related [operator](#highlight.tags.operator).
  */
  typeOperator: $(_t),
  /**
  Control-flow [operator](#highlight.tags.operator).
  */
  controlOperator: $(_t),
  /**
  Program or markup punctuation.
  */
  punctuation: wl,
  /**
  [Punctuation](#highlight.tags.punctuation) that separates
  things.
  */
  separator: $(wl),
  /**
  Bracket-style [punctuation](#highlight.tags.punctuation).
  */
  bracket: In,
  /**
  Angle [brackets](#highlight.tags.bracket) (usually `<` and `>`
  tokens).
  */
  angleBracket: $(In),
  /**
  Square [brackets](#highlight.tags.bracket) (usually `[` and `]`
  tokens).
  */
  squareBracket: $(In),
  /**
  Parentheses (usually `(` and `)` tokens). Subtag of
  [bracket](#highlight.tags.bracket).
  */
  paren: $(In),
  /**
  Braces (usually `{` and `}` tokens). Subtag of
  [bracket](#highlight.tags.bracket).
  */
  brace: $(In),
  /**
  Content, for example plain text in XML or markup documents.
  */
  content: jt,
  /**
  [Content](#highlight.tags.content) that represents a heading.
  */
  heading: Di,
  /**
  A level 1 [heading](#highlight.tags.heading).
  */
  heading1: $(Di),
  /**
  A level 2 [heading](#highlight.tags.heading).
  */
  heading2: $(Di),
  /**
  A level 3 [heading](#highlight.tags.heading).
  */
  heading3: $(Di),
  /**
  A level 4 [heading](#highlight.tags.heading).
  */
  heading4: $(Di),
  /**
  A level 5 [heading](#highlight.tags.heading).
  */
  heading5: $(Di),
  /**
  A level 6 [heading](#highlight.tags.heading).
  */
  heading6: $(Di),
  /**
  A prose [content](#highlight.tags.content) separator (such as a horizontal rule).
  */
  contentSeparator: $(jt),
  /**
  [Content](#highlight.tags.content) that represents a list.
  */
  list: $(jt),
  /**
  [Content](#highlight.tags.content) that represents a quote.
  */
  quote: $(jt),
  /**
  [Content](#highlight.tags.content) that is emphasized.
  */
  emphasis: $(jt),
  /**
  [Content](#highlight.tags.content) that is styled strong.
  */
  strong: $(jt),
  /**
  [Content](#highlight.tags.content) that is part of a link.
  */
  link: $(jt),
  /**
  [Content](#highlight.tags.content) that is styled as code or
  monospace.
  */
  monospace: $(jt),
  /**
  [Content](#highlight.tags.content) that has a strike-through
  style.
  */
  strikethrough: $(jt),
  /**
  Inserted text in a change-tracking format.
  */
  inserted: $(),
  /**
  Deleted text.
  */
  deleted: $(),
  /**
  Changed text.
  */
  changed: $(),
  /**
  An invalid or unsyntactic element.
  */
  invalid: $(),
  /**
  Metadata or meta-instruction.
  */
  meta: Wr,
  /**
  [Metadata](#highlight.tags.meta) that applies to the entire
  document.
  */
  documentMeta: $(Wr),
  /**
  [Metadata](#highlight.tags.meta) that annotates or adds
  attributes to a given syntactic element.
  */
  annotation: $(Wr),
  /**
  Processing instruction or preprocessor directive. Subtag of
  [meta](#highlight.tags.meta).
  */
  processingInstruction: $(Wr),
  /**
  [Modifier](#highlight.Tag^defineModifier) that indicates that a
  given element is being defined. Expected to be used with the
  various [name](#highlight.tags.name) tags.
  */
  definition: Ct.defineModifier("definition"),
  /**
  [Modifier](#highlight.Tag^defineModifier) that indicates that
  something is constant. Mostly expected to be used with
  [variable names](#highlight.tags.variableName).
  */
  constant: Ct.defineModifier("constant"),
  /**
  [Modifier](#highlight.Tag^defineModifier) used to indicate that
  a [variable](#highlight.tags.variableName) or [property
  name](#highlight.tags.propertyName) is being called or defined
  as a function.
  */
  function: Ct.defineModifier("function"),
  /**
  [Modifier](#highlight.Tag^defineModifier) that can be applied to
  [names](#highlight.tags.name) to indicate that they belong to
  the language's standard environment.
  */
  standard: Ct.defineModifier("standard"),
  /**
  [Modifier](#highlight.Tag^defineModifier) that indicates a given
  [names](#highlight.tags.name) is local to some scope.
  */
  local: Ct.defineModifier("local"),
  /**
  A generic variant [modifier](#highlight.Tag^defineModifier) that
  can be used to tag language-specific alternative variants of
  some common tag. It is recommended for themes to define special
  forms of at least the [string](#highlight.tags.string) and
  [variable name](#highlight.tags.variableName) tags, since those
  come up a lot.
  */
  special: Ct.defineModifier("special")
};
for (let n in P) {
  let e = P[n];
  e instanceof Ct && (e.name = n);
}
Ju([
  { tag: P.link, class: "tok-link" },
  { tag: P.heading, class: "tok-heading" },
  { tag: P.emphasis, class: "tok-emphasis" },
  { tag: P.strong, class: "tok-strong" },
  { tag: P.keyword, class: "tok-keyword" },
  { tag: P.atom, class: "tok-atom" },
  { tag: P.bool, class: "tok-bool" },
  { tag: P.url, class: "tok-url" },
  { tag: P.labelName, class: "tok-labelName" },
  { tag: P.inserted, class: "tok-inserted" },
  { tag: P.deleted, class: "tok-deleted" },
  { tag: P.literal, class: "tok-literal" },
  { tag: P.string, class: "tok-string" },
  { tag: P.number, class: "tok-number" },
  { tag: [P.regexp, P.escape, P.special(P.string)], class: "tok-string2" },
  { tag: P.variableName, class: "tok-variableName" },
  { tag: P.local(P.variableName), class: "tok-variableName tok-local" },
  { tag: P.definition(P.variableName), class: "tok-variableName tok-definition" },
  { tag: P.special(P.variableName), class: "tok-variableName2" },
  { tag: P.definition(P.propertyName), class: "tok-propertyName tok-definition" },
  { tag: P.typeName, class: "tok-typeName" },
  { tag: P.namespace, class: "tok-namespace" },
  { tag: P.className, class: "tok-className" },
  { tag: P.macroName, class: "tok-macroName" },
  { tag: P.propertyName, class: "tok-propertyName" },
  { tag: P.operator, class: "tok-operator" },
  { tag: P.comment, class: "tok-comment" },
  { tag: P.meta, class: "tok-meta" },
  { tag: P.invalid, class: "tok-invalid" },
  { tag: P.punctuation, class: "tok-punctuation" }
]);
var Co;
const ln = /* @__PURE__ */ new oe();
function oy(n) {
  return U.define({
    combine: n ? (e) => e.concat(n) : void 0
  });
}
const ly = /* @__PURE__ */ new oe();
class Mt {
  /**
  Construct a language object. If you need to invoke this
  directly, first define a data facet with
  [`defineLanguageFacet`](https://codemirror.net/6/docs/ref/#language.defineLanguageFacet), and then
  configure your parser to [attach](https://codemirror.net/6/docs/ref/#language.languageDataProp) it
  to the language's outer syntax node.
  */
  constructor(e, t, i = [], r = "") {
    this.data = e, this.name = r, pe.prototype.hasOwnProperty("tree") || Object.defineProperty(pe.prototype, "tree", { get() {
      return Ve(this);
    } }), this.parser = t, this.extension = [
      Ai.of(this),
      pe.languageData.of((s, o, l) => {
        let a = kc(s, o, l), f = a.type.prop(ln);
        if (!f)
          return [];
        let u = s.facet(f), g = a.type.prop(ly);
        if (g) {
          let y = a.resolve(o - a.from, l);
          for (let b of g)
            if (b.test(y, s)) {
              let k = s.facet(b.facet);
              return b.type == "replace" ? k : k.concat(u);
            }
        }
        return u;
      })
    ].concat(i);
  }
  /**
  Query whether this language is active at the given position.
  */
  isActiveAt(e, t, i = -1) {
    return kc(e, t, i).type.prop(ln) == this.data;
  }
  /**
  Find the document regions that were parsed using this language.
  The returned regions will _include_ any nested languages rooted
  in this language, when those exist.
  */
  findRegions(e) {
    let t = e.facet(Ai);
    if (t?.data == this.data)
      return [{ from: 0, to: e.doc.length }];
    if (!t || !t.allowsNesting)
      return [];
    let i = [], r = (s, o) => {
      if (s.prop(ln) == this.data) {
        i.push({ from: o, to: o + s.length });
        return;
      }
      let l = s.prop(oe.mounted);
      if (l) {
        if (l.tree.prop(ln) == this.data) {
          if (l.overlay)
            for (let a of l.overlay)
              i.push({ from: a.from + o, to: a.to + o });
          else
            i.push({ from: o, to: o + s.length });
          return;
        } else if (l.overlay) {
          let a = i.length;
          if (r(l.tree, l.overlay[0].from + o), i.length > a)
            return;
        }
      }
      for (let a = 0; a < s.children.length; a++) {
        let f = s.children[a];
        f instanceof Te && r(f, s.positions[a] + o);
      }
    };
    return r(Ve(e), 0), i;
  }
  /**
  Indicates whether this language allows nested languages. The
  default implementation returns true.
  */
  get allowsNesting() {
    return !0;
  }
}
Mt.setState = /* @__PURE__ */ ne.define();
function kc(n, e, t) {
  let i = n.facet(Ai), r = Ve(n).topNode;
  if (!i || i.allowsNesting)
    for (let s = r; s; s = s.enter(e, t, be.ExcludeBuffers | be.EnterBracketed))
      s.type.isTop && (r = s);
  return r;
}
class Ui extends Mt {
  constructor(e, t, i) {
    super(e, t, [], i), this.parser = t;
  }
  /**
  Define a language from a parser.
  */
  static define(e) {
    let t = oy(e.languageData);
    return new Ui(t, e.parser.configure({
      props: [ln.add((i) => i.isTop ? t : void 0)]
    }), e.name);
  }
  /**
  Create a new instance of this language with a reconfigured
  version of its parser and optionally a new name.
  */
  configure(e, t) {
    return new Ui(this.data, this.parser.configure(e), t || this.name);
  }
  get allowsNesting() {
    return this.parser.hasWrappers();
  }
}
function Ve(n) {
  let e = n.field(Mt.state, !1);
  return e ? e.tree : Te.empty;
}
function ed(n, e, t = 50) {
  var i;
  let r = (i = n.field(Mt.state, !1)) === null || i === void 0 ? void 0 : i.context;
  if (!r)
    return null;
  let s = r.viewport;
  r.updateViewport({ from: 0, to: e });
  let o = r.isDone(e) || r.work(t, e) ? r.tree : null;
  return r.updateViewport(s), o;
}
class ay {
  /**
  Create an input object for the given document.
  */
  constructor(e) {
    this.doc = e, this.cursorPos = 0, this.string = "", this.cursor = e.iter();
  }
  get length() {
    return this.doc.length;
  }
  syncTo(e) {
    return this.string = this.cursor.next(e - this.cursorPos).value, this.cursorPos = e + this.string.length, this.cursorPos - this.string.length;
  }
  chunk(e) {
    return this.syncTo(e), this.string;
  }
  get lineChunks() {
    return !0;
  }
  read(e, t) {
    let i = this.cursorPos - this.string.length;
    return e < i || t >= this.cursorPos ? this.doc.sliceString(e, t) : this.string.slice(e - i, t - i);
  }
}
let Nn = null;
class gs {
  constructor(e, t, i = [], r, s, o, l, a) {
    this.parser = e, this.state = t, this.fragments = i, this.tree = r, this.treeLen = s, this.viewport = o, this.skipped = l, this.scheduleOn = a, this.parse = null, this.tempSkipped = [];
  }
  /**
  @internal
  */
  static create(e, t, i) {
    return new gs(e, t, [], Te.empty, 0, i, [], null);
  }
  startParse() {
    return this.parser.startParse(new ay(this.state.doc), this.fragments);
  }
  /**
  @internal
  */
  work(e, t) {
    return t != null && t >= this.state.doc.length && (t = void 0), this.tree != Te.empty && this.isDone(t ?? this.state.doc.length) ? (this.takeTree(), !0) : this.withContext(() => {
      var i;
      if (typeof e == "number") {
        let r = Date.now() + e;
        e = () => Date.now() > r;
      }
      for (this.parse || (this.parse = this.startParse()), t != null && (this.parse.stoppedAt == null || this.parse.stoppedAt > t) && t < this.state.doc.length && this.parse.stopAt(t); ; ) {
        let r = this.parse.advance();
        if (r)
          if (this.fragments = this.withoutTempSkipped(oi.addTree(r, this.fragments, this.parse.stoppedAt != null)), this.treeLen = (i = this.parse.stoppedAt) !== null && i !== void 0 ? i : this.state.doc.length, this.tree = r, this.parse = null, this.treeLen < (t ?? this.state.doc.length))
            this.parse = this.startParse();
          else
            return !0;
        if (e())
          return !1;
      }
    });
  }
  /**
  @internal
  */
  takeTree() {
    let e, t;
    this.parse && (e = this.parse.parsedPos) >= this.treeLen && ((this.parse.stoppedAt == null || this.parse.stoppedAt > e) && this.parse.stopAt(e), this.withContext(() => {
      for (; !(t = this.parse.advance()); )
        ;
    }), this.treeLen = e, this.tree = t, this.fragments = this.withoutTempSkipped(oi.addTree(this.tree, this.fragments, !0)), this.parse = null);
  }
  withContext(e) {
    let t = Nn;
    Nn = this;
    try {
      return e();
    } finally {
      Nn = t;
    }
  }
  withoutTempSkipped(e) {
    for (let t; t = this.tempSkipped.pop(); )
      e = wc(e, t.from, t.to);
    return e;
  }
  /**
  @internal
  */
  changes(e, t) {
    let { fragments: i, tree: r, treeLen: s, viewport: o, skipped: l } = this;
    if (this.takeTree(), !e.empty) {
      let a = [];
      if (e.iterChangedRanges((f, u, g, y) => a.push({ fromA: f, toA: u, fromB: g, toB: y })), i = oi.applyChanges(i, a), r = Te.empty, s = 0, o = { from: e.mapPos(o.from, -1), to: e.mapPos(o.to, 1) }, this.skipped.length) {
        l = [];
        for (let f of this.skipped) {
          let u = e.mapPos(f.from, 1), g = e.mapPos(f.to, -1);
          u < g && l.push({ from: u, to: g });
        }
      }
    }
    return new gs(this.parser, t, i, r, s, o, l, this.scheduleOn);
  }
  /**
  @internal
  */
  updateViewport(e) {
    if (this.viewport.from == e.from && this.viewport.to == e.to)
      return !1;
    this.viewport = e;
    let t = this.skipped.length;
    for (let i = 0; i < this.skipped.length; i++) {
      let { from: r, to: s } = this.skipped[i];
      r < e.to && s > e.from && (this.fragments = wc(this.fragments, r, s), this.skipped.splice(i--, 1));
    }
    return this.skipped.length >= t ? !1 : (this.reset(), !0);
  }
  /**
  @internal
  */
  reset() {
    this.parse && (this.takeTree(), this.parse = null);
  }
  /**
  Notify the parse scheduler that the given region was skipped
  because it wasn't in view, and the parse should be restarted
  when it comes into view.
  */
  skipUntilInView(e, t) {
    this.skipped.push({ from: e, to: t });
  }
  /**
  Returns a parser intended to be used as placeholder when
  asynchronously loading a nested parser. It'll skip its input and
  mark it as not-really-parsed, so that the next update will parse
  it again.
  
  When `until` is given, a reparse will be scheduled when that
  promise resolves.
  */
  static getSkippingParser(e) {
    return new class extends Gu {
      createParse(t, i, r) {
        let s = r[0].from, o = r[r.length - 1].to;
        return {
          parsedPos: s,
          advance() {
            let a = Nn;
            if (a) {
              for (let f of r)
                a.tempSkipped.push(f);
              e && (a.scheduleOn = a.scheduleOn ? Promise.all([a.scheduleOn, e]) : e);
            }
            return this.parsedPos = o, new Te(ot.none, [], [], o - s);
          },
          stoppedAt: null,
          stopAt() {
          }
        };
      }
    }();
  }
  /**
  @internal
  */
  isDone(e) {
    e = Math.min(e, this.state.doc.length);
    let t = this.fragments;
    return this.treeLen >= e && t.length && t[0].from == 0 && t[0].to >= e;
  }
  /**
  Get the context for the current parse, or `null` if no editor
  parse is in progress.
  */
  static get() {
    return Nn;
  }
}
function wc(n, e, t) {
  return oi.applyChanges(n, [{ fromA: e, toA: t, fromB: e, toB: t }]);
}
class Sn {
  constructor(e) {
    this.context = e, this.tree = e.tree;
  }
  apply(e) {
    if (!e.docChanged && this.tree == this.context.tree)
      return this;
    let t = this.context.changes(e.changes, e.state), i = this.context.treeLen == e.startState.doc.length ? void 0 : Math.max(e.changes.mapPos(this.context.treeLen), t.viewport.to);
    return t.work(20, i) || t.takeTree(), new Sn(t);
  }
  static init(e) {
    let t = Math.min(3e3, e.doc.length), i = gs.create(e.facet(Ai).parser, e, { from: 0, to: t });
    return i.work(20, t) || i.takeTree(), new Sn(i);
  }
}
Mt.state = /* @__PURE__ */ $e.define({
  create: Sn.init,
  update(n, e) {
    for (let t of e.effects)
      if (t.is(Mt.setState))
        return t.value;
    return e.startState.facet(Ai) != e.state.facet(Ai) ? Sn.init(e.state) : n.apply(e);
  }
});
let td = (n) => {
  let e = setTimeout(
    () => n(),
    500
    /* Work.MaxPause */
  );
  return () => clearTimeout(e);
};
typeof requestIdleCallback < "u" && (td = (n) => {
  let e = -1, t = setTimeout(
    () => {
      e = requestIdleCallback(n, {
        timeout: 400
        /* Work.MinPause */
      });
    },
    100
    /* Work.MinPause */
  );
  return () => e < 0 ? clearTimeout(t) : cancelIdleCallback(e);
});
const Oo = typeof navigator < "u" && (!((Co = navigator.scheduling) === null || Co === void 0) && Co.isInputPending) ? () => navigator.scheduling.isInputPending() : null, hy = /* @__PURE__ */ De.fromClass(class {
  constructor(e) {
    this.view = e, this.working = null, this.workScheduled = 0, this.chunkEnd = -1, this.chunkBudget = -1, this.work = this.work.bind(this), this.scheduleWork();
  }
  update(e) {
    let t = this.view.state.field(Mt.state).context;
    (t.updateViewport(e.view.viewport) || this.view.viewport.to > t.treeLen) && this.scheduleWork(), (e.docChanged || e.selectionSet) && (this.view.hasFocus && (this.chunkBudget += 50), this.scheduleWork()), this.checkAsyncSchedule(t);
  }
  scheduleWork() {
    if (this.working)
      return;
    let { state: e } = this.view, t = e.field(Mt.state);
    (t.tree != t.context.tree || !t.context.isDone(e.doc.length)) && (this.working = td(this.work));
  }
  work(e) {
    this.working = null;
    let t = Date.now();
    if (this.chunkEnd < t && (this.chunkEnd < 0 || this.view.hasFocus) && (this.chunkEnd = t + 3e4, this.chunkBudget = 3e3), this.chunkBudget <= 0)
      return;
    let { state: i, viewport: { to: r } } = this.view, s = i.field(Mt.state);
    if (s.tree == s.context.tree && s.context.isDone(
      r + 1e5
      /* Work.MaxParseAhead */
    ))
      return;
    let o = Date.now() + Math.min(this.chunkBudget, 100, e && !Oo ? Math.max(25, e.timeRemaining() - 5) : 1e9), l = s.context.treeLen < r && i.doc.length > r + 1e3, a = s.context.work(() => Oo && Oo() || Date.now() > o, r + (l ? 0 : 1e5));
    this.chunkBudget -= Date.now() - t, (a || this.chunkBudget <= 0) && (s.context.takeTree(), this.view.dispatch({ effects: Mt.setState.of(new Sn(s.context)) })), this.chunkBudget > 0 && !(a && !l) && this.scheduleWork(), this.checkAsyncSchedule(s.context);
  }
  checkAsyncSchedule(e) {
    e.scheduleOn && (this.workScheduled++, e.scheduleOn.then(() => this.scheduleWork()).catch((t) => ft(this.view.state, t)).then(() => this.workScheduled--), e.scheduleOn = null);
  }
  destroy() {
    this.working && this.working();
  }
  isWorking() {
    return !!(this.working || this.workScheduled > 0);
  }
}, {
  eventHandlers: { focus() {
    this.scheduleWork();
  } }
}), Ai = /* @__PURE__ */ U.define({
  combine(n) {
    return n.length ? n[0] : null;
  },
  enables: (n) => [
    Mt.state,
    hy,
    _.contentAttributes.compute([n], (e) => {
      let t = e.facet(n);
      return t && t.name ? { "data-language": t.name } : {};
    })
  ]
});
class ha {
  /**
  Create a language support object.
  */
  constructor(e, t = []) {
    this.language = e, this.support = t, this.extension = [e, t];
  }
}
const cy = /* @__PURE__ */ U.define(), ir = /* @__PURE__ */ U.define({
  combine: (n) => {
    if (!n.length)
      return "  ";
    let e = n[0];
    if (!e || /\S/.test(e) || Array.from(e).some((t) => t != e[0]))
      throw new Error("Invalid indent unit: " + JSON.stringify(n[0]));
    return e;
  }
});
function ms(n) {
  let e = n.facet(ir);
  return e.charCodeAt(0) == 9 ? n.tabSize * e.length : e.length;
}
function nr(n, e) {
  let t = "", i = n.tabSize, r = n.facet(ir)[0];
  if (r == "	") {
    for (; e >= i; )
      t += "	", e -= i;
    r = " ";
  }
  for (let s = 0; s < e; s++)
    t += r;
  return t;
}
function ca(n, e) {
  n instanceof pe && (n = new Vs(n));
  for (let i of n.state.facet(cy)) {
    let r = i(n, e);
    if (r !== void 0)
      return r;
  }
  let t = Ve(n.state);
  return t.length >= e ? fy(n, t, e) : null;
}
class Vs {
  /**
  Create an indent context.
  */
  constructor(e, t = {}) {
    this.state = e, this.options = t, this.unit = ms(e);
  }
  /**
  Get a description of the line at the given position, taking
  [simulated line
  breaks](https://codemirror.net/6/docs/ref/#language.IndentContext.constructor^options.simulateBreak)
  into account. If there is such a break at `pos`, the `bias`
  argument determines whether the part of the line line before or
  after the break is used.
  */
  lineAt(e, t = 1) {
    let i = this.state.doc.lineAt(e), { simulateBreak: r, simulateDoubleBreak: s } = this.options;
    return r != null && r >= i.from && r <= i.to ? s && r == e ? { text: "", from: e } : (t < 0 ? r < e : r <= e) ? { text: i.text.slice(r - i.from), from: r } : { text: i.text.slice(0, r - i.from), from: i.from } : i;
  }
  /**
  Get the text directly after `pos`, either the entire line
  or the next 100 characters, whichever is shorter.
  */
  textAfterPos(e, t = 1) {
    if (this.options.simulateDoubleBreak && e == this.options.simulateBreak)
      return "";
    let { text: i, from: r } = this.lineAt(e, t);
    return i.slice(e - r, Math.min(i.length, e + 100 - r));
  }
  /**
  Find the column for the given position.
  */
  column(e, t = 1) {
    let { text: i, from: r } = this.lineAt(e, t), s = this.countColumn(i, e - r), o = this.options.overrideIndentation ? this.options.overrideIndentation(r) : -1;
    return o > -1 && (s += o - this.countColumn(i, i.search(/\S|$/))), s;
  }
  /**
  Find the column position (taking tabs into account) of the given
  position in the given string.
  */
  countColumn(e, t = e.length) {
    return Mn(e, this.state.tabSize, t);
  }
  /**
  Find the indentation column of the line at the given point.
  */
  lineIndent(e, t = 1) {
    let { text: i, from: r } = this.lineAt(e, t), s = this.options.overrideIndentation;
    if (s) {
      let o = s(r);
      if (o > -1)
        return o;
    }
    return this.countColumn(i, i.search(/\S|$/));
  }
  /**
  Returns the [simulated line
  break](https://codemirror.net/6/docs/ref/#language.IndentContext.constructor^options.simulateBreak)
  for this context, if any.
  */
  get simulatedBreak() {
    return this.options.simulateBreak || null;
  }
}
const Hs = /* @__PURE__ */ new oe();
function fy(n, e, t) {
  let i = e.resolveStack(t), r = e.resolveInner(t, -1).resolve(t, 0).enterUnfinishedNodesBefore(t);
  if (r != i.node) {
    let s = [];
    for (let o = r; o && !(o.from < i.node.from || o.to > i.node.to || o.from == i.node.from && o.type == i.node.type); o = o.parent)
      s.push(o);
    for (let o = s.length - 1; o >= 0; o--)
      i = { node: s[o], next: i };
  }
  return id(i, n, t);
}
function id(n, e, t) {
  for (let i = n; i; i = i.next) {
    let r = dy(i.node);
    if (r)
      return r(fa.create(e, t, i));
  }
  return 0;
}
function uy(n) {
  return n.pos == n.options.simulateBreak && n.options.simulateDoubleBreak;
}
function dy(n) {
  let e = n.type.prop(Hs);
  if (e)
    return e;
  let t = n.firstChild, i;
  if (t && (i = t.type.prop(oe.closedBy))) {
    let r = n.lastChild, s = r && i.indexOf(r.name) > -1;
    return (o) => vy(o, !0, 1, void 0, s && !uy(o) ? r.from : void 0);
  }
  return n.parent == null ? py : null;
}
function py() {
  return 0;
}
class fa extends Vs {
  constructor(e, t, i) {
    super(e.state, e.options), this.base = e, this.pos = t, this.context = i;
  }
  /**
  The syntax tree node to which the indentation strategy
  applies.
  */
  get node() {
    return this.context.node;
  }
  /**
  @internal
  */
  static create(e, t, i) {
    return new fa(e, t, i);
  }
  /**
  Get the text directly after `this.pos`, either the entire line
  or the next 100 characters, whichever is shorter.
  */
  get textAfter() {
    return this.textAfterPos(this.pos);
  }
  /**
  Get the indentation at the reference line for `this.node`, which
  is the line on which it starts, unless there is a node that is
  _not_ a parent of this node covering the start of that line. If
  so, the line at the start of that node is tried, again skipping
  on if it is covered by another such node.
  */
  get baseIndent() {
    return this.baseIndentFor(this.node);
  }
  /**
  Get the indentation for the reference line of the given node
  (see [`baseIndent`](https://codemirror.net/6/docs/ref/#language.TreeIndentContext.baseIndent)).
  */
  baseIndentFor(e) {
    let t = this.state.doc.lineAt(e.from);
    for (; ; ) {
      let i = e.resolve(t.from);
      for (; i.parent && i.parent.from == i.from; )
        i = i.parent;
      if (gy(i, e))
        break;
      t = this.state.doc.lineAt(i.from);
    }
    return this.lineIndent(t.from);
  }
  /**
  Continue looking for indentations in the node's parent nodes,
  and return the result of that.
  */
  continue() {
    return id(this.context.next, this.base, this.pos);
  }
}
function gy(n, e) {
  for (let t = e; t; t = t.parent)
    if (n == t)
      return !0;
  return !1;
}
function my(n) {
  let e = n.node, t = e.childAfter(e.from), i = e.lastChild;
  if (!t)
    return null;
  let r = n.options.simulateBreak, s = n.state.doc.lineAt(t.from), o = r == null || r <= s.from ? s.to : Math.min(s.to, r);
  for (let l = t.to; ; ) {
    let a = e.childAfter(l);
    if (!a || a == i)
      return null;
    if (!a.type.isSkipped) {
      if (a.from >= o)
        return null;
      let f = /^ */.exec(s.text.slice(t.to - s.from))[0].length;
      return { from: t.from, to: t.to + f };
    }
    l = a.to;
  }
}
function vy(n, e, t, i, r) {
  let s = n.textAfter, o = s.match(/^\s*/)[0].length, l = i && s.slice(o, o + i.length) == i || r == n.pos + o, a = my(n);
  return a ? l ? n.column(a.from) : n.column(a.to) : n.baseIndent + (l ? 0 : n.unit * t);
}
function rr({ except: n, units: e = 1 } = {}) {
  return (t) => {
    let i = n && n.test(t.textAfter);
    return t.baseIndent + (i ? 0 : e * t.unit);
  };
}
const yy = 200;
function by() {
  return pe.transactionFilter.of((n) => {
    if (!n.docChanged || !n.isUserEvent("input.type") && !n.isUserEvent("input.complete"))
      return n;
    let e = n.startState.languageDataAt("indentOnInput", n.startState.selection.main.head);
    if (!e.length)
      return n;
    let t = n.newDoc, { head: i } = n.newSelection.main, r = t.lineAt(i);
    if (i > r.from + yy)
      return n;
    let s = t.sliceString(r.from, i);
    if (!e.some((f) => f.test(s)))
      return n;
    let { state: o } = n, l = -1, a = [];
    for (let { head: f } of o.selection.ranges) {
      let u = o.doc.lineAt(f);
      if (u.from == l)
        continue;
      l = u.from;
      let g = ca(o, u.from);
      if (g == null)
        continue;
      let y = /^\s*/.exec(u.text)[0], b = nr(o, g);
      y != b && a.push({ from: u.from, to: u.from + y.length, insert: b });
    }
    return a.length ? [n, { changes: a, sequential: !0 }] : n;
  });
}
const nd = /* @__PURE__ */ U.define(), zs = /* @__PURE__ */ new oe();
function Sl(n) {
  let e = n.firstChild, t = n.lastChild;
  return e && e.to < t.from ? { from: e.to, to: t.type.isError ? n.to : t.from } : null;
}
function xy(n, e, t) {
  let i = Ve(n);
  if (i.length < t)
    return null;
  let r = i.resolveStack(t, 1), s = null;
  for (let o = r; o; o = o.next) {
    let l = o.node;
    if (l.to <= t || l.from > t)
      continue;
    if (s && l.from < e)
      break;
    let a = l.type.prop(zs);
    if (a && (l.to < i.length - 50 || i.length == n.doc.length || !ky(l))) {
      let f = a(l, n);
      f && f.from <= t && f.from >= e && f.to > t && (s = f);
    }
  }
  return s;
}
function ky(n) {
  let e = n.lastChild;
  return e && e.to == n.to && e.type.isError;
}
function vs(n, e, t) {
  for (let i of n.facet(nd)) {
    let r = i(n, e, t);
    if (r)
      return r;
  }
  return xy(n, e, t);
}
function rd(n, e) {
  let t = e.mapPos(n.from, 1), i = e.mapPos(n.to, -1);
  return t >= i ? void 0 : { from: t, to: i };
}
const $s = /* @__PURE__ */ ne.define({ map: rd }), pr = /* @__PURE__ */ ne.define({ map: rd });
function sd(n) {
  let e = [];
  for (let { head: t } of n.state.selection.ranges)
    e.some((i) => i.from <= t && i.to >= t) || e.push(n.lineBlockAt(t));
  return e;
}
const Xi = /* @__PURE__ */ $e.define({
  create() {
    return G.none;
  },
  update(n, e) {
    e.isUserEvent("delete") && e.changes.iterChangedRanges((t, i) => n = Sc(n, t, i)), n = n.map(e.changes);
    for (let t of e.effects)
      if (t.is($s) && !wy(n, t.value.from, t.value.to)) {
        let { preparePlaceholder: i } = e.state.facet(hd), r = i ? G.replace({ widget: new Ty(i(e.state, t.value)) }) : Cc;
        n = n.update({ add: [r.range(t.value.from, t.value.to)] });
      } else t.is(pr) && (n = n.update({
        filter: (i, r) => t.value.from != i || t.value.to != r,
        filterFrom: t.value.from,
        filterTo: t.value.to
      }));
    return e.selection && (n = Sc(n, e.selection.main.head)), n;
  },
  provide: (n) => _.decorations.from(n),
  toJSON(n, e) {
    let t = [];
    return n.between(0, e.doc.length, (i, r) => {
      t.push(i, r);
    }), t;
  },
  fromJSON(n) {
    if (!Array.isArray(n) || n.length % 2)
      throw new RangeError("Invalid JSON for fold state");
    let e = [];
    for (let t = 0; t < n.length; ) {
      let i = n[t++], r = n[t++];
      if (typeof i != "number" || typeof r != "number")
        throw new RangeError("Invalid JSON for fold state");
      e.push(Cc.range(i, r));
    }
    return G.set(e, !0);
  }
});
function Sc(n, e, t = e) {
  let i = !1;
  return n.between(e, t, (r, s) => {
    r < t && s > e && (i = !0);
  }), i ? n.update({
    filterFrom: e,
    filterTo: t,
    filter: (r, s) => r >= t || s <= e
  }) : n;
}
function ys(n, e, t) {
  var i;
  let r = null;
  return (i = n.field(Xi, !1)) === null || i === void 0 || i.between(e, t, (s, o) => {
    (!r || r.from > s) && (r = { from: s, to: o });
  }), r;
}
function wy(n, e, t) {
  let i = !1;
  return n.between(e, e, (r, s) => {
    r == e && s == t && (i = !0);
  }), i;
}
function od(n, e) {
  return n.field(Xi, !1) ? e : e.concat(ne.appendConfig.of(cd()));
}
const ld = (n) => {
  for (let e of sd(n)) {
    let t = vs(n.state, e.from, e.to);
    if (t)
      return n.dispatch({ effects: od(n.state, [$s.of(t), ad(n, t)]) }), !0;
  }
  return !1;
}, Sy = (n) => {
  if (!n.state.field(Xi, !1))
    return !1;
  let e = [];
  for (let t of sd(n)) {
    let i = ys(n.state, t.from, t.to);
    i && e.push(pr.of(i), ad(n, i, !1));
  }
  return e.length && n.dispatch({ effects: e }), e.length > 0;
};
function ad(n, e, t = !0) {
  let i = n.state.doc.lineAt(e.from).number, r = n.state.doc.lineAt(e.to).number;
  return _.announce.of(`${n.state.phrase(t ? "Folded lines" : "Unfolded lines")} ${i} ${n.state.phrase("to")} ${r}.`);
}
const Cy = (n) => {
  let { state: e } = n, t = [];
  for (let i = 0; i < e.doc.length; ) {
    let r = n.lineBlockAt(i), s = vs(e, r.from, r.to);
    s && t.push($s.of(s)), i = (s ? n.lineBlockAt(s.to) : r).to + 1;
  }
  return t.length && n.dispatch({ effects: od(n.state, t) }), !!t.length;
}, Oy = (n) => {
  let e = n.state.field(Xi, !1);
  if (!e || !e.size)
    return !1;
  let t = [];
  return e.between(0, n.state.doc.length, (i, r) => {
    t.push(pr.of({ from: i, to: r }));
  }), n.dispatch({ effects: t }), !0;
}, Ay = [
  { key: "Ctrl-Shift-[", mac: "Cmd-Alt-[", run: ld },
  { key: "Ctrl-Shift-]", mac: "Cmd-Alt-]", run: Sy },
  { key: "Ctrl-Alt-[", run: Cy },
  { key: "Ctrl-Alt-]", run: Oy }
], My = {
  placeholderDOM: null,
  preparePlaceholder: null,
  placeholderText: "…"
}, hd = /* @__PURE__ */ U.define({
  combine(n) {
    return ti(n, My);
  }
});
function cd(n) {
  return [Xi, Ly];
}
function fd(n, e) {
  let { state: t } = n, i = t.facet(hd), r = (o) => {
    let l = n.lineBlockAt(n.posAtDOM(o.target)), a = ys(n.state, l.from, l.to);
    a && n.dispatch({ effects: pr.of(a) }), o.preventDefault();
  };
  if (i.placeholderDOM)
    return i.placeholderDOM(n, r, e);
  let s = document.createElement("span");
  return s.textContent = i.placeholderText, s.setAttribute("aria-label", t.phrase("folded code")), s.title = t.phrase("unfold"), s.className = "cm-foldPlaceholder", s.onclick = r, s;
}
const Cc = /* @__PURE__ */ G.replace({ widget: /* @__PURE__ */ new class extends ci {
  toDOM(n) {
    return fd(n, null);
  }
}() });
class Ty extends ci {
  constructor(e) {
    super(), this.value = e;
  }
  eq(e) {
    return this.value == e.value;
  }
  toDOM(e) {
    return fd(e, this.value);
  }
}
const Py = {
  openText: "⌄",
  closedText: "›",
  markerDOM: null,
  domEventHandlers: {},
  foldingChanged: () => !1
};
class Ao extends ai {
  constructor(e, t) {
    super(), this.config = e, this.open = t;
  }
  eq(e) {
    return this.config == e.config && this.open == e.open;
  }
  toDOM(e) {
    if (this.config.markerDOM)
      return this.config.markerDOM(this.open);
    let t = document.createElement("span");
    return t.textContent = this.open ? this.config.openText : this.config.closedText, t.title = e.state.phrase(this.open ? "Fold line" : "Unfold line"), t;
  }
}
function Ry(n = {}) {
  let e = { ...Py, ...n }, t = new Ao(e, !0), i = new Ao(e, !1), r = De.fromClass(class {
    constructor(o) {
      this.from = o.viewport.from, this.markers = this.buildMarkers(o);
    }
    update(o) {
      (o.docChanged || o.viewportChanged || o.startState.facet(Ai) != o.state.facet(Ai) || o.startState.field(Xi, !1) != o.state.field(Xi, !1) || Ve(o.startState) != Ve(o.state) || e.foldingChanged(o)) && (this.markers = this.buildMarkers(o.view));
    }
    buildMarkers(o) {
      let l = new ei();
      for (let a of o.viewportLineBlocks) {
        let f = ys(o.state, a.from, a.to) ? i : vs(o.state, a.from, a.to) ? t : null;
        f && l.add(a.from, a.from, f);
      }
      return l.finish();
    }
  }), { domEventHandlers: s } = e;
  return [
    r,
    Mv({
      class: "cm-foldGutter",
      markers(o) {
        var l;
        return ((l = o.plugin(r)) === null || l === void 0 ? void 0 : l.markers) || ce.empty;
      },
      initialSpacer() {
        return new Ao(e, !1);
      },
      domEventHandlers: {
        ...s,
        click: (o, l, a) => {
          if (s.click && s.click(o, l, a))
            return !0;
          let f = ys(o.state, l.from, l.to);
          if (f)
            return o.dispatch({ effects: pr.of(f) }), !0;
          let u = vs(o.state, l.from, l.to);
          return u ? (o.dispatch({ effects: $s.of(u) }), !0) : !1;
        }
      }
    }),
    cd()
  ];
}
const Ly = /* @__PURE__ */ _.baseTheme({
  ".cm-foldPlaceholder": {
    backgroundColor: "#eee",
    border: "1px solid #ddd",
    color: "#888",
    borderRadius: ".2em",
    margin: "0 1px",
    padding: "0 1px",
    cursor: "pointer"
  },
  ".cm-foldGutter span": {
    padding: "0 1px",
    cursor: "pointer"
  }
});
class gr {
  constructor(e, t) {
    this.specs = e;
    let i;
    function r(l) {
      let a = wi.newName();
      return (i || (i = /* @__PURE__ */ Object.create(null)))["." + a] = l, a;
    }
    const s = typeof t.all == "string" ? t.all : t.all ? r(t.all) : void 0, o = t.scope;
    this.scope = o instanceof Mt ? (l) => l.prop(ln) == o.data : o ? (l) => l == o : void 0, this.style = Ju(e.map((l) => ({
      tag: l.tag,
      class: l.class || r(Object.assign({}, l, { tag: null }))
    })), {
      all: s
    }).style, this.module = i ? new wi(i) : null, this.themeType = t.themeType;
  }
  /**
  Create a highlighter style that associates the given styles to
  the given tags. The specs must be objects that hold a style tag
  or array of tags in their `tag` property, and either a single
  `class` property providing a static CSS class (for highlighter
  that rely on external styling), or a
  [`style-mod`](https://github.com/marijnh/style-mod#documentation)-style
  set of CSS properties (which define the styling for those tags).
  
  The CSS rules created for a highlighter will be emitted in the
  order of the spec's properties. That means that for elements that
  have multiple tags associated with them, styles defined further
  down in the list will have a higher CSS precedence than styles
  defined earlier.
  */
  static define(e, t) {
    return new gr(e, t || {});
  }
}
const Cl = /* @__PURE__ */ U.define(), ud = /* @__PURE__ */ U.define({
  combine(n) {
    return n.length ? [n[0]] : null;
  }
});
function Mo(n) {
  let e = n.facet(Cl);
  return e.length ? e : n.facet(ud);
}
function dd(n, e) {
  let t = [By], i;
  return n instanceof gr && (n.module && t.push(_.styleModule.of(n.module)), i = n.themeType), e?.fallback ? t.push(ud.of(n)) : i ? t.push(Cl.computeN([_.darkTheme], (r) => r.facet(_.darkTheme) == (i == "dark") ? [n] : [])) : t.push(Cl.of(n)), t;
}
class Dy {
  constructor(e) {
    this.markCache = /* @__PURE__ */ Object.create(null), this.tree = Ve(e.state), this.decorations = this.buildDeco(e, Mo(e.state)), this.decoratedTo = e.viewport.to;
  }
  update(e) {
    let t = Ve(e.state), i = Mo(e.state), r = i != Mo(e.startState), { viewport: s } = e.view, o = e.changes.mapPos(this.decoratedTo, 1);
    t.length < s.to && !r && t.type == this.tree.type && o >= s.to ? (this.decorations = this.decorations.map(e.changes), this.decoratedTo = o) : (t != this.tree || e.viewportChanged || r) && (this.tree = t, this.decorations = this.buildDeco(e.view, i), this.decoratedTo = s.to);
  }
  buildDeco(e, t) {
    if (!t || !this.tree.length)
      return G.none;
    let i = new ei();
    for (let { from: r, to: s } of e.visibleRanges)
      ny(this.tree, t, (o, l, a) => {
        i.add(o, l, this.markCache[a] || (this.markCache[a] = G.mark({ class: a })));
      }, r, s);
    return i.finish();
  }
}
const By = /* @__PURE__ */ Ti.high(/* @__PURE__ */ De.fromClass(Dy, {
  decorations: (n) => n.decorations
})), Ey = /* @__PURE__ */ gr.define([
  {
    tag: P.meta,
    color: "#404740"
  },
  {
    tag: P.link,
    textDecoration: "underline"
  },
  {
    tag: P.heading,
    textDecoration: "underline",
    fontWeight: "bold"
  },
  {
    tag: P.emphasis,
    fontStyle: "italic"
  },
  {
    tag: P.strong,
    fontWeight: "bold"
  },
  {
    tag: P.strikethrough,
    textDecoration: "line-through"
  },
  {
    tag: P.keyword,
    color: "#708"
  },
  {
    tag: [P.atom, P.bool, P.url, P.contentSeparator, P.labelName],
    color: "#219"
  },
  {
    tag: [P.literal, P.inserted],
    color: "#164"
  },
  {
    tag: [P.string, P.deleted],
    color: "#a11"
  },
  {
    tag: [P.regexp, P.escape, /* @__PURE__ */ P.special(P.string)],
    color: "#e40"
  },
  {
    tag: /* @__PURE__ */ P.definition(P.variableName),
    color: "#00f"
  },
  {
    tag: /* @__PURE__ */ P.local(P.variableName),
    color: "#30a"
  },
  {
    tag: [P.typeName, P.namespace],
    color: "#085"
  },
  {
    tag: P.className,
    color: "#167"
  },
  {
    tag: [/* @__PURE__ */ P.special(P.variableName), P.macroName],
    color: "#256"
  },
  {
    tag: /* @__PURE__ */ P.definition(P.propertyName),
    color: "#00c"
  },
  {
    tag: P.comment,
    color: "#940"
  },
  {
    tag: P.invalid,
    color: "#f00"
  }
]), Iy = /* @__PURE__ */ _.baseTheme({
  "&.cm-focused .cm-matchingBracket": { backgroundColor: "#328c8252" },
  "&.cm-focused .cm-nonmatchingBracket": { backgroundColor: "#bb555544" }
}), pd = 1e4, gd = "()[]{}", md = /* @__PURE__ */ U.define({
  combine(n) {
    return ti(n, {
      afterCursor: !0,
      brackets: gd,
      maxScanDistance: pd,
      renderMatch: Wy
    });
  }
}), Ny = /* @__PURE__ */ G.mark({ class: "cm-matchingBracket" }), Fy = /* @__PURE__ */ G.mark({ class: "cm-nonmatchingBracket" });
function Wy(n) {
  let e = [], t = n.matched ? Ny : Fy;
  return e.push(t.range(n.start.from, n.start.to)), n.end && e.push(t.range(n.end.from, n.end.to)), e;
}
const Qy = /* @__PURE__ */ $e.define({
  create() {
    return G.none;
  },
  update(n, e) {
    if (!e.docChanged && !e.selection)
      return n;
    let t = [], i = e.state.facet(md);
    for (let r of e.state.selection.ranges) {
      if (!r.empty)
        continue;
      let s = Tt(e.state, r.head, -1, i) || r.head > 0 && Tt(e.state, r.head - 1, 1, i) || i.afterCursor && (Tt(e.state, r.head, 1, i) || r.head < e.state.doc.length && Tt(e.state, r.head + 1, -1, i));
      s && (t = t.concat(i.renderMatch(s, e.state)));
    }
    return G.set(t, !0);
  },
  provide: (n) => _.decorations.from(n)
}), Vy = [
  Qy,
  Iy
];
function Hy(n = {}) {
  return [md.of(n), Vy];
}
const zy = /* @__PURE__ */ new oe();
function Ol(n, e, t) {
  let i = n.prop(e < 0 ? oe.openedBy : oe.closedBy);
  if (i)
    return i;
  if (n.name.length == 1) {
    let r = t.indexOf(n.name);
    if (r > -1 && r % 2 == (e < 0 ? 1 : 0))
      return [t[r + e]];
  }
  return null;
}
function Al(n) {
  let e = n.type.prop(zy);
  return e ? e(n.node) : n;
}
function Tt(n, e, t, i = {}) {
  let r = i.maxScanDistance || pd, s = i.brackets || gd, o = Ve(n), l = o.resolveInner(e, t);
  for (let a = l; a; a = a.parent) {
    let f = Ol(a.type, t, s);
    if (f && a.from < a.to) {
      let u = Al(a);
      if (u && (t > 0 ? e >= u.from && e < u.to : e > u.from && e <= u.to))
        return $y(n, e, t, a, u, f, s);
    }
  }
  return qy(n, e, t, o, l.type, r, s);
}
function $y(n, e, t, i, r, s, o) {
  let l = i.parent, a = { from: r.from, to: r.to }, f = 0, u = l?.cursor();
  if (u && (t < 0 ? u.childBefore(i.from) : u.childAfter(i.to)))
    do
      if (t < 0 ? u.to <= i.from : u.from >= i.to) {
        if (f == 0 && s.indexOf(u.type.name) > -1 && u.from < u.to) {
          let g = Al(u);
          return { start: a, end: g ? { from: g.from, to: g.to } : void 0, matched: !0 };
        } else if (Ol(u.type, t, o))
          f++;
        else if (Ol(u.type, -t, o)) {
          if (f == 0) {
            let g = Al(u);
            return {
              start: a,
              end: g && g.from < g.to ? { from: g.from, to: g.to } : void 0,
              matched: !1
            };
          }
          f--;
        }
      }
    while (t < 0 ? u.prevSibling() : u.nextSibling());
  return { start: a, matched: !1 };
}
function qy(n, e, t, i, r, s, o) {
  let l = t < 0 ? n.sliceDoc(e - 1, e) : n.sliceDoc(e, e + 1), a = o.indexOf(l);
  if (a < 0 || a % 2 == 0 != t > 0)
    return null;
  let f = { from: t < 0 ? e - 1 : e, to: t > 0 ? e + 1 : e }, u = n.doc.iterRange(e, t > 0 ? n.doc.length : 0), g = 0;
  for (let y = 0; !u.next().done && y <= s; ) {
    let b = u.value;
    t < 0 && (y += b.length);
    let k = e + y * t;
    for (let C = t > 0 ? 0 : b.length - 1, M = t > 0 ? b.length : -1; C != M; C += t) {
      let R = o.indexOf(b[C]);
      if (!(R < 0 || i.resolveInner(k + C, 1).type != r))
        if (R % 2 == 0 == t > 0)
          g++;
        else {
          if (g == 1)
            return { start: f, end: { from: k + C, to: k + C + 1 }, matched: R >> 1 == a >> 1 };
          g--;
        }
    }
    t > 0 && (y += b.length);
  }
  return u.done ? { start: f, matched: !1 } : null;
}
function Oc(n, e, t, i = 0, r = 0) {
  e == null && (e = n.search(/[^\s\u00a0]/), e == -1 && (e = n.length));
  let s = r;
  for (let o = i; o < e; o++)
    n.charCodeAt(o) == 9 ? s += t - s % t : s++;
  return s;
}
class Ky {
  /**
  Create a stream.
  */
  constructor(e, t, i, r) {
    this.string = e, this.tabSize = t, this.indentUnit = i, this.overrideIndent = r, this.pos = 0, this.start = 0, this.lastColumnPos = 0, this.lastColumnValue = 0;
  }
  /**
  True if we are at the end of the line.
  */
  eol() {
    return this.pos >= this.string.length;
  }
  /**
  True if we are at the start of the line.
  */
  sol() {
    return this.pos == 0;
  }
  /**
  Get the next code unit after the current position, or undefined
  if we're at the end of the line.
  */
  peek() {
    return this.string.charAt(this.pos) || void 0;
  }
  /**
  Read the next code unit and advance `this.pos`.
  */
  next() {
    if (this.pos < this.string.length)
      return this.string.charAt(this.pos++);
  }
  /**
  Match the next character against the given string, regular
  expression, or predicate. Consume and return it if it matches.
  */
  eat(e) {
    let t = this.string.charAt(this.pos), i;
    if (typeof e == "string" ? i = t == e : i = t && (e instanceof RegExp ? e.test(t) : e(t)), i)
      return ++this.pos, t;
  }
  /**
  Continue matching characters that match the given string,
  regular expression, or predicate function. Return true if any
  characters were consumed.
  */
  eatWhile(e) {
    let t = this.pos;
    for (; this.eat(e); )
      ;
    return this.pos > t;
  }
  /**
  Consume whitespace ahead of `this.pos`. Return true if any was
  found.
  */
  eatSpace() {
    let e = this.pos;
    for (; /[\s\u00a0]/.test(this.string.charAt(this.pos)); )
      ++this.pos;
    return this.pos > e;
  }
  /**
  Move to the end of the line.
  */
  skipToEnd() {
    this.pos = this.string.length;
  }
  /**
  Move to directly before the given character, if found on the
  current line.
  */
  skipTo(e) {
    let t = this.string.indexOf(e, this.pos);
    if (t > -1)
      return this.pos = t, !0;
  }
  /**
  Move back `n` characters.
  */
  backUp(e) {
    this.pos -= e;
  }
  /**
  Get the column position at `this.pos`.
  */
  column() {
    return this.lastColumnPos < this.start && (this.lastColumnValue = Oc(this.string, this.start, this.tabSize, this.lastColumnPos, this.lastColumnValue), this.lastColumnPos = this.start), this.lastColumnValue;
  }
  /**
  Get the indentation column of the current line.
  */
  indentation() {
    var e;
    return (e = this.overrideIndent) !== null && e !== void 0 ? e : Oc(this.string, null, this.tabSize);
  }
  /**
  Match the input against the given string or regular expression
  (which should start with a `^`). Return true or the regexp match
  if it matches.
  
  Unless `consume` is set to `false`, this will move `this.pos`
  past the matched text.
  
  When matching a string `caseInsensitive` can be set to true to
  make the match case-insensitive.
  */
  match(e, t, i) {
    if (typeof e == "string") {
      let r = (o) => i ? o.toLowerCase() : o, s = this.string.substr(this.pos, e.length);
      return r(s) == r(e) ? (t !== !1 && (this.pos += e.length), !0) : null;
    } else {
      let r = this.string.slice(this.pos).match(e);
      return r && r.index > 0 ? null : (r && t !== !1 && (this.pos += r[0].length), r);
    }
  }
  /**
  Get the current token.
  */
  current() {
    return this.string.slice(this.start, this.pos);
  }
}
const _y = /* @__PURE__ */ Object.create(null), Ac = [ot.none], Mc = [], Tc = /* @__PURE__ */ Object.create(null), jy = /* @__PURE__ */ Object.create(null);
for (let [n, e] of [
  ["variable", "variableName"],
  ["variable-2", "variableName.special"],
  ["string-2", "string.special"],
  ["def", "variableName.definition"],
  ["tag", "tagName"],
  ["attribute", "attributeName"],
  ["type", "typeName"],
  ["builtin", "variableName.standard"],
  ["qualifier", "modifier"],
  ["error", "invalid"],
  ["header", "heading"],
  ["property", "propertyName"]
])
  jy[n] = /* @__PURE__ */ Uy(_y, e);
function To(n, e) {
  Mc.indexOf(n) > -1 || (Mc.push(n), console.warn(e));
}
function Uy(n, e) {
  let t = [];
  for (let l of e.split(" ")) {
    let a = [];
    for (let f of l.split(".")) {
      let u = n[f] || P[f];
      u ? typeof u == "function" ? a.length ? a = a.map(u) : To(f, `Modifier ${f} used at start of tag`) : a.length ? To(f, `Tag ${f} used as modifier`) : a = Array.isArray(u) ? u : [u] : To(f, `Unknown highlighting tag ${f}`);
    }
    for (let f of a)
      t.push(f);
  }
  if (!t.length)
    return 0;
  let i = e.replace(/ /g, "_"), r = i + " " + t.map((l) => l.id), s = Tc[r];
  if (s)
    return s.id;
  let o = Tc[r] = ot.define({
    id: Ac.length,
    name: i,
    props: [Qs({ [i]: t })]
  });
  return Ac.push(o), o.id;
}
xe.RTL, xe.LTR;
const Xy = (n) => {
  let { state: e } = n, t = e.doc.lineAt(e.selection.main.from), i = da(n.state, t.from);
  return i.line ? Yy(n) : i.block ? Zy(n) : !1;
};
function ua(n, e) {
  return ({ state: t, dispatch: i }) => {
    if (t.readOnly)
      return !1;
    let r = n(e, t);
    return r ? (i(t.update(r)), !0) : !1;
  };
}
const Yy = /* @__PURE__ */ ua(
  tb,
  0
  /* CommentOption.Toggle */
), Gy = /* @__PURE__ */ ua(
  vd,
  0
  /* CommentOption.Toggle */
), Zy = /* @__PURE__ */ ua(
  (n, e) => vd(n, e, eb(e)),
  0
  /* CommentOption.Toggle */
);
function da(n, e) {
  let t = n.languageDataAt("commentTokens", e, 1);
  return t.length ? t[0] : {};
}
const Fn = 50;
function Jy(n, { open: e, close: t }, i, r) {
  let s = n.sliceDoc(i - Fn, i), o = n.sliceDoc(r, r + Fn), l = /\s*$/.exec(s)[0].length, a = /^\s*/.exec(o)[0].length, f = s.length - l;
  if (s.slice(f - e.length, f) == e && o.slice(a, a + t.length) == t)
    return {
      open: { pos: i - l, margin: l && 1 },
      close: { pos: r + a, margin: a && 1 }
    };
  let u, g;
  r - i <= 2 * Fn ? u = g = n.sliceDoc(i, r) : (u = n.sliceDoc(i, i + Fn), g = n.sliceDoc(r - Fn, r));
  let y = /^\s*/.exec(u)[0].length, b = /\s*$/.exec(g)[0].length, k = g.length - b - t.length;
  return u.slice(y, y + e.length) == e && g.slice(k, k + t.length) == t ? {
    open: {
      pos: i + y + e.length,
      margin: /\s/.test(u.charAt(y + e.length)) ? 1 : 0
    },
    close: {
      pos: r - b - t.length,
      margin: /\s/.test(g.charAt(k - 1)) ? 1 : 0
    }
  } : null;
}
function eb(n) {
  let e = [];
  for (let t of n.selection.ranges) {
    let i = n.doc.lineAt(t.from), r = t.to <= i.to ? i : n.doc.lineAt(t.to);
    r.from > i.from && r.from == t.to && (r = t.to == i.to + 1 ? i : n.doc.lineAt(t.to - 1));
    let s = e.length - 1;
    s >= 0 && e[s].to > i.from ? e[s].to = r.to : e.push({ from: i.from + /^\s*/.exec(i.text)[0].length, to: r.to });
  }
  return e;
}
function vd(n, e, t = e.selection.ranges) {
  let i = t.map((s) => da(e, s.from).block);
  if (!i.every((s) => s))
    return null;
  let r = t.map((s, o) => Jy(e, i[o], s.from, s.to));
  if (n != 2 && !r.every((s) => s))
    return { changes: e.changes(t.map((s, o) => r[o] ? [] : [{ from: s.from, insert: i[o].open + " " }, { from: s.to, insert: " " + i[o].close }])) };
  if (n != 1 && r.some((s) => s)) {
    let s = [];
    for (let o = 0, l; o < r.length; o++)
      if (l = r[o]) {
        let a = i[o], { open: f, close: u } = l;
        s.push({ from: f.pos - a.open.length, to: f.pos + f.margin }, { from: u.pos - u.margin, to: u.pos + a.close.length });
      }
    return { changes: s };
  }
  return null;
}
function tb(n, e, t = e.selection.ranges) {
  let i = [], r = -1;
  for (let { from: s, to: o } of t) {
    let l = i.length, a = 1e9, f = da(e, s).line;
    if (f) {
      for (let u = s; u <= o; ) {
        let g = e.doc.lineAt(u);
        if (g.from > r && (s == o || o > g.from)) {
          r = g.from;
          let y = /^\s*/.exec(g.text)[0].length, b = y == g.length, k = g.text.slice(y, y + f.length) == f ? y : -1;
          y < g.text.length && y < a && (a = y), i.push({ line: g, comment: k, token: f, indent: y, empty: b, single: !1 });
        }
        u = g.to + 1;
      }
      if (a < 1e9)
        for (let u = l; u < i.length; u++)
          i[u].indent < i[u].line.text.length && (i[u].indent = a);
      i.length == l + 1 && (i[l].single = !0);
    }
  }
  if (n != 2 && i.some((s) => s.comment < 0 && (!s.empty || s.single))) {
    let s = [];
    for (let { line: l, token: a, indent: f, empty: u, single: g } of i)
      (g || !u) && s.push({ from: l.from + f, insert: a + " " });
    let o = e.changes(s);
    return { changes: o, selection: e.selection.map(o, 1) };
  } else if (n != 1 && i.some((s) => s.comment >= 0)) {
    let s = [];
    for (let { line: o, comment: l, token: a } of i)
      if (l >= 0) {
        let f = o.from + l, u = f + a.length;
        o.text[u - o.from] == " " && u++, s.push({ from: f, to: u });
      }
    return { changes: s };
  }
  return null;
}
const Ml = /* @__PURE__ */ hi.define(), ib = /* @__PURE__ */ hi.define(), nb = /* @__PURE__ */ U.define(), yd = /* @__PURE__ */ U.define({
  combine(n) {
    return ti(n, {
      minDepth: 100,
      newGroupDelay: 500,
      joinToEvent: (e, t) => t
    }, {
      minDepth: Math.max,
      newGroupDelay: Math.min,
      joinToEvent: (e, t) => (i, r) => e(i, r) || t(i, r)
    });
  }
}), bd = /* @__PURE__ */ $e.define({
  create() {
    return Zt.empty;
  },
  update(n, e) {
    let t = e.state.facet(yd), i = e.annotation(Ml);
    if (i) {
      let a = ut.fromTransaction(e, i.selection), f = i.side, u = f == 0 ? n.undone : n.done;
      return a ? u = xs(u, u.length, t.minDepth, a) : u = kd(u, e.startState.selection), new Zt(f == 0 ? i.rest : u, f == 0 ? u : i.rest);
    }
    let r = e.annotation(ib);
    if ((r == "full" || r == "before") && (n = n.isolate()), e.annotation(Qe.addToHistory) === !1)
      return e.changes.empty ? n : n.addMapping(e.changes.desc);
    let s = ut.fromTransaction(e), o = e.annotation(Qe.time), l = e.annotation(Qe.userEvent);
    return s ? n = n.addChanges(s, o, l, t, e) : e.selection && (n = n.addSelection(e.startState.selection, o, l, t.newGroupDelay)), (r == "full" || r == "after") && (n = n.isolate()), n;
  },
  toJSON(n) {
    return { done: n.done.map((e) => e.toJSON()), undone: n.undone.map((e) => e.toJSON()) };
  },
  fromJSON(n) {
    return new Zt(n.done.map(ut.fromJSON), n.undone.map(ut.fromJSON));
  }
});
function rb(n = {}) {
  return [
    bd,
    yd.of(n),
    _.domEventHandlers({
      beforeinput(e, t) {
        let i = e.inputType == "historyUndo" ? pa : e.inputType == "historyRedo" ? bs : null;
        return i ? (e.preventDefault(), i(t)) : !1;
      }
    })
  ];
}
function qs(n, e) {
  return function({ state: t, dispatch: i }) {
    if (!e && t.readOnly)
      return !1;
    let r = t.field(bd, !1);
    if (!r)
      return !1;
    let s = r.pop(n, t, e);
    return s ? (i(s), !0) : !1;
  };
}
const pa = /* @__PURE__ */ qs(0, !1), bs = /* @__PURE__ */ qs(1, !1), sb = /* @__PURE__ */ qs(0, !0), ob = /* @__PURE__ */ qs(1, !0);
class ut {
  constructor(e, t, i, r, s) {
    this.changes = e, this.effects = t, this.mapped = i, this.startSelection = r, this.selectionsAfter = s;
  }
  setSelAfter(e) {
    return new ut(this.changes, this.effects, this.mapped, this.startSelection, e);
  }
  toJSON() {
    var e, t, i;
    return {
      changes: (e = this.changes) === null || e === void 0 ? void 0 : e.toJSON(),
      mapped: (t = this.mapped) === null || t === void 0 ? void 0 : t.toJSON(),
      startSelection: (i = this.startSelection) === null || i === void 0 ? void 0 : i.toJSON(),
      selectionsAfter: this.selectionsAfter.map((r) => r.toJSON())
    };
  }
  static fromJSON(e) {
    return new ut(e.changes && Fe.fromJSON(e.changes), [], e.mapped && Jt.fromJSON(e.mapped), e.startSelection && E.fromJSON(e.startSelection), e.selectionsAfter.map(E.fromJSON));
  }
  // This does not check `addToHistory` and such, it assumes the
  // transaction needs to be converted to an item. Returns null when
  // there are no changes or effects in the transaction.
  static fromTransaction(e, t) {
    let i = Pt;
    for (let r of e.startState.facet(nb)) {
      let s = r(e);
      s.length && (i = i.concat(s));
    }
    return !i.length && e.changes.empty ? null : new ut(e.changes.invert(e.startState.doc), i, void 0, t || e.startState.selection, Pt);
  }
  static selection(e) {
    return new ut(void 0, Pt, void 0, void 0, e);
  }
}
function xs(n, e, t, i) {
  let r = e + 1 > t + 20 ? e - t - 1 : 0, s = n.slice(r, e);
  return s.push(i), s;
}
function lb(n, e) {
  let t = [], i = !1;
  return n.iterChangedRanges((r, s) => t.push(r, s)), e.iterChangedRanges((r, s, o, l) => {
    for (let a = 0; a < t.length; ) {
      let f = t[a++], u = t[a++];
      l >= f && o <= u && (i = !0);
    }
  }), i;
}
function ab(n, e) {
  return n.ranges.length == e.ranges.length && n.ranges.filter((t, i) => t.empty != e.ranges[i].empty).length === 0;
}
function xd(n, e) {
  return n.length ? e.length ? n.concat(e) : n : e;
}
const Pt = [], hb = 200;
function kd(n, e) {
  if (n.length) {
    let t = n[n.length - 1], i = t.selectionsAfter.slice(Math.max(0, t.selectionsAfter.length - hb));
    return i.length && i[i.length - 1].eq(e) ? n : (i.push(e), xs(n, n.length - 1, 1e9, t.setSelAfter(i)));
  } else
    return [ut.selection([e])];
}
function cb(n) {
  let e = n[n.length - 1], t = n.slice();
  return t[n.length - 1] = e.setSelAfter(e.selectionsAfter.slice(0, e.selectionsAfter.length - 1)), t;
}
function Po(n, e) {
  if (!n.length)
    return n;
  let t = n.length, i = Pt;
  for (; t; ) {
    let r = fb(n[t - 1], e, i);
    if (r.changes && !r.changes.empty || r.effects.length) {
      let s = n.slice(0, t);
      return s[t - 1] = r, s;
    } else
      e = r.mapped, t--, i = r.selectionsAfter;
  }
  return i.length ? [ut.selection(i)] : Pt;
}
function fb(n, e, t) {
  let i = xd(n.selectionsAfter.length ? n.selectionsAfter.map((l) => l.map(e)) : Pt, t);
  if (!n.changes)
    return ut.selection(i);
  let r = n.changes.map(e), s = e.mapDesc(n.changes, !0), o = n.mapped ? n.mapped.composeDesc(s) : s;
  return new ut(r, ne.mapEffects(n.effects, e), o, n.startSelection.map(s), i);
}
const ub = /^(input\.type|delete)($|\.)/;
class Zt {
  constructor(e, t, i = 0, r = void 0) {
    this.done = e, this.undone = t, this.prevTime = i, this.prevUserEvent = r;
  }
  isolate() {
    return this.prevTime ? new Zt(this.done, this.undone) : this;
  }
  addChanges(e, t, i, r, s) {
    let o = this.done, l = o[o.length - 1];
    return l && l.changes && !l.changes.empty && e.changes && (!i || ub.test(i)) && (!l.selectionsAfter.length && t - this.prevTime < r.newGroupDelay && r.joinToEvent(s, lb(l.changes, e.changes)) || // For compose (but not compose.start) events, always join with previous event
    i == "input.type.compose") ? o = xs(o, o.length - 1, r.minDepth, new ut(e.changes.compose(l.changes), xd(ne.mapEffects(e.effects, l.changes), l.effects), l.mapped, l.startSelection, Pt)) : o = xs(o, o.length, r.minDepth, e), new Zt(o, Pt, t, i);
  }
  addSelection(e, t, i, r) {
    let s = this.done.length ? this.done[this.done.length - 1].selectionsAfter : Pt;
    return s.length > 0 && t - this.prevTime < r && i == this.prevUserEvent && i && /^select($|\.)/.test(i) && ab(s[s.length - 1], e) ? this : new Zt(kd(this.done, e), this.undone, t, i);
  }
  addMapping(e) {
    return new Zt(Po(this.done, e), Po(this.undone, e), this.prevTime, this.prevUserEvent);
  }
  pop(e, t, i) {
    let r = e == 0 ? this.done : this.undone;
    if (r.length == 0)
      return null;
    let s = r[r.length - 1], o = s.selectionsAfter[0] || t.selection;
    if (i && s.selectionsAfter.length)
      return t.update({
        selection: s.selectionsAfter[s.selectionsAfter.length - 1],
        annotations: Ml.of({ side: e, rest: cb(r), selection: o }),
        userEvent: e == 0 ? "select.undo" : "select.redo",
        scrollIntoView: !0
      });
    if (s.changes) {
      let l = r.length == 1 ? Pt : r.slice(0, r.length - 1);
      return s.mapped && (l = Po(l, s.mapped)), t.update({
        changes: s.changes,
        selection: s.startSelection,
        effects: s.effects,
        annotations: Ml.of({ side: e, rest: l, selection: o }),
        filter: !1,
        userEvent: e == 0 ? "undo" : "redo",
        scrollIntoView: !0
      });
    } else
      return null;
  }
}
Zt.empty = /* @__PURE__ */ new Zt(Pt, Pt);
const db = [
  { key: "Mod-z", run: pa, preventDefault: !0 },
  { key: "Mod-y", mac: "Mod-Shift-z", run: bs, preventDefault: !0 },
  { linux: "Ctrl-Shift-z", run: bs, preventDefault: !0 },
  { key: "Mod-u", run: sb, preventDefault: !0 },
  { key: "Alt-u", mac: "Mod-Shift-u", run: ob, preventDefault: !0 }
];
function Tn(n, e) {
  return E.create(n.ranges.map(e), n.mainIndex);
}
function Wt(n, e) {
  return n.update({ selection: e, scrollIntoView: !0, userEvent: "select" });
}
function Qt({ state: n, dispatch: e }, t) {
  let i = Tn(n.selection, t);
  return i.eq(n.selection, !0) ? !1 : (e(Wt(n, i)), !0);
}
function Ks(n, e) {
  return E.cursor(e ? n.to : n.from);
}
function ga(n, e) {
  return Qt(n, (t) => t.empty ? n.moveByChar(t, e) : Ks(t, e));
}
function et(n) {
  return n.textDirectionAt(n.state.selection.main.head) == xe.LTR;
}
const ma = (n) => ga(n, !et(n)), wd = (n) => ga(n, et(n)), pb = (n) => ga(n, !1);
function Sd(n, e) {
  return Qt(n, (t) => t.empty ? n.moveByGroup(t, e) : Ks(t, e));
}
const gb = (n) => Sd(n, !et(n)), mb = (n) => Sd(n, et(n));
function vb(n, e, t) {
  if (e.type.prop(t))
    return !0;
  let i = e.to - e.from;
  return i && (i > 2 || /[^\s,.;:]/.test(n.sliceDoc(e.from, e.to))) || e.firstChild;
}
function _s(n, e, t) {
  let i = Ve(n).resolveInner(e.head), r = t ? oe.closedBy : oe.openedBy;
  for (let a = e.head; ; ) {
    let f = t ? i.childAfter(a) : i.childBefore(a);
    if (!f)
      break;
    vb(n, f, r) ? i = f : a = t ? f.to : f.from;
  }
  let s = i.type.prop(r), o, l;
  return s && (o = t ? Tt(n, i.from, 1) : Tt(n, i.to, -1)) && o.matched ? l = t ? o.end.to : o.end.from : l = t ? i.to : i.from, E.cursor(l, t ? -1 : 1);
}
const yb = (n) => Qt(n, (e) => _s(n.state, e, !et(n))), bb = (n) => Qt(n, (e) => _s(n.state, e, et(n)));
function Cd(n, e) {
  return Qt(n, (t) => {
    if (!t.empty)
      return Ks(t, e);
    let i = n.moveVertically(t, e);
    return i.head != t.head ? i : n.moveToLineBoundary(t, e);
  });
}
const Od = (n) => Cd(n, !1), Ad = (n) => Cd(n, !0);
function Md(n) {
  let e = n.scrollDOM.clientHeight < n.scrollDOM.scrollHeight - 2, t = 0, i = 0, r;
  if (e) {
    for (let s of n.state.facet(_.scrollMargins)) {
      let o = s(n);
      o?.top && (t = Math.max(o?.top, t)), o?.bottom && (i = Math.max(o?.bottom, i));
    }
    r = n.scrollDOM.clientHeight - t - i;
  } else
    r = (n.dom.ownerDocument.defaultView || window).innerHeight;
  return {
    marginTop: t,
    marginBottom: i,
    selfScroll: e,
    height: Math.max(n.defaultLineHeight, r - 5)
  };
}
function Td(n, e) {
  let t = Md(n), { state: i } = n, r = Tn(i.selection, (o) => o.empty ? n.moveVertically(o, e, t.height) : Ks(o, e));
  if (r.eq(i.selection))
    return !1;
  let s;
  if (t.selfScroll) {
    let o = n.coordsAtPos(i.selection.main.head), l = n.scrollDOM.getBoundingClientRect(), a = l.top + t.marginTop, f = l.bottom - t.marginBottom;
    o && o.top > a && o.bottom < f && (s = _.scrollIntoView(r.main.head, { y: "start", yMargin: o.top - a }));
  }
  return n.dispatch(Wt(i, r), { effects: s }), !0;
}
const Pc = (n) => Td(n, !1), Tl = (n) => Td(n, !0);
function Pi(n, e, t) {
  let i = n.lineBlockAt(e.head), r = n.moveToLineBoundary(e, t);
  if (r.head == e.head && r.head != (t ? i.to : i.from) && (r = n.moveToLineBoundary(e, t, !1)), !t && r.head == i.from && i.length) {
    let s = /^\s*/.exec(n.state.sliceDoc(i.from, Math.min(i.from + 100, i.to)))[0].length;
    s && e.head != i.from + s && (r = E.cursor(i.from + s));
  }
  return r;
}
const Pd = (n) => Qt(n, (e) => Pi(n, e, !0)), Rd = (n) => Qt(n, (e) => Pi(n, e, !1)), xb = (n) => Qt(n, (e) => Pi(n, e, !et(n))), kb = (n) => Qt(n, (e) => Pi(n, e, et(n))), wb = (n) => Qt(n, (e) => E.cursor(n.lineBlockAt(e.head).from, 1)), Sb = (n) => Qt(n, (e) => E.cursor(n.lineBlockAt(e.head).to, -1));
function Cb(n, e, t) {
  let i = !1, r = Tn(n.selection, (s) => {
    let o = Tt(n, s.head, -1) || Tt(n, s.head, 1) || s.head > 0 && Tt(n, s.head - 1, 1) || s.head < n.doc.length && Tt(n, s.head + 1, -1);
    if (!o || !o.end)
      return s;
    i = !0;
    let l = o.start.from == s.head ? o.end.to : o.end.from;
    return E.cursor(l);
  });
  return i ? (e(Wt(n, r)), !0) : !1;
}
const Ob = ({ state: n, dispatch: e }) => Cb(n, e);
function Dt(n, e) {
  let t = Tn(n.state.selection, (i) => {
    let r = e(i);
    return E.range(i.anchor, r.head, r.goalColumn, r.bidiLevel || void 0);
  });
  return t.eq(n.state.selection) ? !1 : (n.dispatch(Wt(n.state, t)), !0);
}
function Ld(n, e) {
  return Dt(n, (t) => n.moveByChar(t, e));
}
const Dd = (n) => Ld(n, !et(n)), Bd = (n) => Ld(n, et(n));
function Ed(n, e) {
  return Dt(n, (t) => n.moveByGroup(t, e));
}
const Ab = (n) => Ed(n, !et(n)), Mb = (n) => Ed(n, et(n)), Tb = (n) => Dt(n, (e) => _s(n.state, e, !et(n))), Pb = (n) => Dt(n, (e) => _s(n.state, e, et(n)));
function Id(n, e) {
  return Dt(n, (t) => n.moveVertically(t, e));
}
const Nd = (n) => Id(n, !1), Fd = (n) => Id(n, !0);
function Wd(n, e) {
  return Dt(n, (t) => n.moveVertically(t, e, Md(n).height));
}
const Rc = (n) => Wd(n, !1), Lc = (n) => Wd(n, !0), Rb = (n) => Dt(n, (e) => Pi(n, e, !0)), Lb = (n) => Dt(n, (e) => Pi(n, e, !1)), Db = (n) => Dt(n, (e) => Pi(n, e, !et(n))), Bb = (n) => Dt(n, (e) => Pi(n, e, et(n))), Eb = (n) => Dt(n, (e) => E.cursor(n.lineBlockAt(e.head).from)), Ib = (n) => Dt(n, (e) => E.cursor(n.lineBlockAt(e.head).to)), Dc = ({ state: n, dispatch: e }) => (e(Wt(n, { anchor: 0 })), !0), Bc = ({ state: n, dispatch: e }) => (e(Wt(n, { anchor: n.doc.length })), !0), Ec = ({ state: n, dispatch: e }) => (e(Wt(n, { anchor: n.selection.main.anchor, head: 0 })), !0), Ic = ({ state: n, dispatch: e }) => (e(Wt(n, { anchor: n.selection.main.anchor, head: n.doc.length })), !0), Nb = ({ state: n, dispatch: e }) => (e(n.update({ selection: { anchor: 0, head: n.doc.length }, userEvent: "select" })), !0), Fb = ({ state: n, dispatch: e }) => {
  let t = js(n).map(({ from: i, to: r }) => E.range(i, Math.min(r + 1, n.doc.length)));
  return e(n.update({ selection: E.create(t), userEvent: "select" })), !0;
}, Wb = ({ state: n, dispatch: e }) => {
  let t = Tn(n.selection, (i) => {
    let r = Ve(n), s = r.resolveStack(i.from, 1);
    if (i.empty) {
      let o = r.resolveStack(i.from, -1);
      o.node.from >= s.node.from && o.node.to <= s.node.to && (s = o);
    }
    for (let o = s; o; o = o.next) {
      let { node: l } = o;
      if ((l.from < i.from && l.to >= i.to || l.to > i.to && l.from <= i.from) && o.next)
        return E.range(l.to, l.from);
    }
    return i;
  });
  return t.eq(n.selection) ? !1 : (e(Wt(n, t)), !0);
};
function Qd(n, e) {
  let { state: t } = n, i = t.selection, r = t.selection.ranges.slice();
  for (let s of t.selection.ranges) {
    let o = t.doc.lineAt(s.head);
    if (e ? o.to < n.state.doc.length : o.from > 0)
      for (let l = s; ; ) {
        let a = n.moveVertically(l, e);
        if (a.head < o.from || a.head > o.to) {
          r.some((f) => f.head == a.head) || r.push(a);
          break;
        } else {
          if (a.head == l.head)
            break;
          l = a;
        }
      }
  }
  return r.length == i.ranges.length ? !1 : (n.dispatch(Wt(t, E.create(r, r.length - 1))), !0);
}
const Qb = (n) => Qd(n, !1), Vb = (n) => Qd(n, !0), Hb = ({ state: n, dispatch: e }) => {
  let t = n.selection, i = null;
  return t.ranges.length > 1 ? i = E.create([t.main]) : t.main.empty || (i = E.create([E.cursor(t.main.head)])), i ? (e(Wt(n, i)), !0) : !1;
};
function mr(n, e) {
  if (n.state.readOnly)
    return !1;
  let t = "delete.selection", { state: i } = n, r = i.changeByRange((s) => {
    let { from: o, to: l } = s;
    if (o == l) {
      let a = e(s);
      a < o ? (t = "delete.backward", a = Qr(n, a, !1)) : a > o && (t = "delete.forward", a = Qr(n, a, !0)), o = Math.min(o, a), l = Math.max(l, a);
    } else
      o = Qr(n, o, !1), l = Qr(n, l, !0);
    return o == l ? { range: s } : { changes: { from: o, to: l }, range: E.cursor(o, o < s.head ? -1 : 1) };
  });
  return r.changes.empty ? !1 : (n.dispatch(i.update(r, {
    scrollIntoView: !0,
    userEvent: t,
    effects: t == "delete.selection" ? _.announce.of(i.phrase("Selection deleted")) : void 0
  })), !0);
}
function Qr(n, e, t) {
  if (n instanceof _)
    for (let i of n.state.facet(_.atomicRanges).map((r) => r(n)))
      i.between(e, e, (r, s) => {
        r < e && s > e && (e = t ? s : r);
      });
  return e;
}
const Vd = (n, e, t) => mr(n, (i) => {
  let r = i.from, { state: s } = n, o = s.doc.lineAt(r), l, a;
  if (t && !e && r > o.from && r < o.from + 200 && !/[^ \t]/.test(l = o.text.slice(0, r - o.from))) {
    if (l[l.length - 1] == "	")
      return r - 1;
    let f = Mn(l, s.tabSize), u = f % ms(s) || ms(s);
    for (let g = 0; g < u && l[l.length - 1 - g] == " "; g++)
      r--;
    a = r;
  } else
    a = We(o.text, r - o.from, e, e) + o.from, a == r && o.number != (e ? s.doc.lines : 1) ? a += e ? 1 : -1 : !e && /[\ufe00-\ufe0f]/.test(o.text.slice(a - o.from, r - o.from)) && (a = We(o.text, a - o.from, !1, !1) + o.from);
  return a;
}), Pl = (n) => Vd(n, !1, !0), Hd = (n) => Vd(n, !0, !1), zd = (n, e) => mr(n, (t) => {
  let i = t.head, { state: r } = n, s = r.doc.lineAt(i), o = r.charCategorizer(i);
  for (let l = null; ; ) {
    if (i == (e ? s.to : s.from)) {
      i == t.head && s.number != (e ? r.doc.lines : 1) && (i += e ? 1 : -1);
      break;
    }
    let a = We(s.text, i - s.from, e) + s.from, f = s.text.slice(Math.min(i, a) - s.from, Math.max(i, a) - s.from), u = o(f);
    if (l != null && u != l)
      break;
    (f != " " || i != t.head) && (l = u), i = a;
  }
  return i;
}), $d = (n) => zd(n, !1), zb = (n) => zd(n, !0), $b = (n) => mr(n, (e) => {
  let t = n.lineBlockAt(e.head).to;
  return e.head < t ? t : Math.min(n.state.doc.length, e.head + 1);
}), qb = (n) => mr(n, (e) => {
  let t = n.moveToLineBoundary(e, !1).head;
  return e.head > t ? t : Math.max(0, e.head - 1);
}), Kb = (n) => mr(n, (e) => {
  let t = n.moveToLineBoundary(e, !0).head;
  return e.head < t ? t : Math.min(n.state.doc.length, e.head + 1);
}), _b = ({ state: n, dispatch: e }) => {
  if (n.readOnly)
    return !1;
  let t = n.changeByRange((i) => ({
    changes: { from: i.from, to: i.to, insert: ge.of(["", ""]) },
    range: E.cursor(i.from)
  }));
  return e(n.update(t, { scrollIntoView: !0, userEvent: "input" })), !0;
}, jb = ({ state: n, dispatch: e }) => {
  if (n.readOnly)
    return !1;
  let t = n.changeByRange((i) => {
    if (!i.empty || i.from == 0 || i.from == n.doc.length)
      return { range: i };
    let r = i.from, s = n.doc.lineAt(r), o = r == s.from ? r - 1 : We(s.text, r - s.from, !1) + s.from, l = r == s.to ? r + 1 : We(s.text, r - s.from, !0) + s.from;
    return {
      changes: { from: o, to: l, insert: n.doc.slice(r, l).append(n.doc.slice(o, r)) },
      range: E.cursor(l)
    };
  });
  return t.changes.empty ? !1 : (e(n.update(t, { scrollIntoView: !0, userEvent: "move.character" })), !0);
};
function js(n) {
  let e = [], t = -1;
  for (let i of n.selection.ranges) {
    let r = n.doc.lineAt(i.from), s = n.doc.lineAt(i.to);
    if (!i.empty && i.to == s.from && (s = n.doc.lineAt(i.to - 1)), t >= r.number) {
      let o = e[e.length - 1];
      o.to = s.to, o.ranges.push(i);
    } else
      e.push({ from: r.from, to: s.to, ranges: [i] });
    t = s.number + 1;
  }
  return e;
}
function qd(n, e, t) {
  if (n.readOnly)
    return !1;
  let i = [], r = [];
  for (let s of js(n)) {
    if (t ? s.to == n.doc.length : s.from == 0)
      continue;
    let o = n.doc.lineAt(t ? s.to + 1 : s.from - 1), l = o.length + 1;
    if (t) {
      i.push({ from: s.to, to: o.to }, { from: s.from, insert: o.text + n.lineBreak });
      for (let a of s.ranges)
        r.push(E.range(Math.min(n.doc.length, a.anchor + l), Math.min(n.doc.length, a.head + l)));
    } else {
      i.push({ from: o.from, to: s.from }, { from: s.to, insert: n.lineBreak + o.text });
      for (let a of s.ranges)
        r.push(E.range(a.anchor - l, a.head - l));
    }
  }
  return i.length ? (e(n.update({
    changes: i,
    scrollIntoView: !0,
    selection: E.create(r, n.selection.mainIndex),
    userEvent: "move.line"
  })), !0) : !1;
}
const Ub = ({ state: n, dispatch: e }) => qd(n, e, !1), Xb = ({ state: n, dispatch: e }) => qd(n, e, !0);
function Kd(n, e, t) {
  if (n.readOnly)
    return !1;
  let i = [];
  for (let s of js(n))
    t ? i.push({ from: s.from, insert: n.doc.slice(s.from, s.to) + n.lineBreak }) : i.push({ from: s.to, insert: n.lineBreak + n.doc.slice(s.from, s.to) });
  let r = n.changes(i);
  return e(n.update({
    changes: r,
    selection: n.selection.map(r, t ? 1 : -1),
    scrollIntoView: !0,
    userEvent: "input.copyline"
  })), !0;
}
const Yb = ({ state: n, dispatch: e }) => Kd(n, e, !1), Gb = ({ state: n, dispatch: e }) => Kd(n, e, !0), Zb = (n) => {
  if (n.state.readOnly)
    return !1;
  let { state: e } = n, t = e.changes(js(e).map(({ from: r, to: s }) => (r > 0 ? r-- : s < e.doc.length && s++, { from: r, to: s }))), i = Tn(e.selection, (r) => {
    let s;
    if (n.lineWrapping) {
      let o = n.lineBlockAt(r.head), l = n.coordsAtPos(r.head, r.assoc || 1);
      l && (s = o.bottom + n.documentTop - l.bottom + n.defaultLineHeight / 2);
    }
    return n.moveVertically(r, !0, s);
  }).map(t);
  return n.dispatch({ changes: t, selection: i, scrollIntoView: !0, userEvent: "delete.line" }), !0;
};
function Jb(n, e) {
  if (/\(\)|\[\]|\{\}/.test(n.sliceDoc(e - 1, e + 1)))
    return { from: e, to: e };
  let t = Ve(n).resolveInner(e), i = t.childBefore(e), r = t.childAfter(e), s;
  return i && r && i.to <= e && r.from >= e && (s = i.type.prop(oe.closedBy)) && s.indexOf(r.name) > -1 && n.doc.lineAt(i.to).from == n.doc.lineAt(r.from).from && !/\S/.test(n.sliceDoc(i.to, r.from)) ? { from: i.to, to: r.from } : null;
}
const Rl = /* @__PURE__ */ _d(!1), ex = /* @__PURE__ */ _d(!0);
function _d(n) {
  return ({ state: e, dispatch: t }) => {
    if (e.readOnly)
      return !1;
    let i = e.changeByRange((r) => {
      let { from: s, to: o } = r, l = e.doc.lineAt(s), a = !n && s == o && Jb(e, s);
      n && (s = o = (o <= l.to ? l : e.doc.lineAt(o)).to);
      let f = new Vs(e, { simulateBreak: s, simulateDoubleBreak: !!a }), u = ca(f, s);
      for (u == null && (u = Mn(/^\s*/.exec(e.doc.lineAt(s).text)[0], e.tabSize)); o < l.to && /\s/.test(l.text[o - l.from]); )
        o++;
      a ? { from: s, to: o } = a : s > l.from && s < l.from + 100 && !/\S/.test(l.text.slice(0, s)) && (s = l.from);
      let g = ["", nr(e, u)];
      return a && g.push(nr(e, f.lineIndent(l.from, -1))), {
        changes: { from: s, to: o, insert: ge.of(g) },
        range: E.cursor(s + 1 + g[1].length)
      };
    });
    return t(e.update(i, { scrollIntoView: !0, userEvent: "input" })), !0;
  };
}
function va(n, e) {
  let t = -1;
  return n.changeByRange((i) => {
    let r = [];
    for (let o = i.from; o <= i.to; ) {
      let l = n.doc.lineAt(o);
      l.number > t && (i.empty || i.to > l.from) && (e(l, r, i), t = l.number), o = l.to + 1;
    }
    let s = n.changes(r);
    return {
      changes: r,
      range: E.range(s.mapPos(i.anchor, 1), s.mapPos(i.head, 1))
    };
  });
}
const jd = ({ state: n, dispatch: e }) => {
  if (n.readOnly)
    return !1;
  let t = /* @__PURE__ */ Object.create(null), i = new Vs(n, { overrideIndentation: (s) => {
    let o = t[s];
    return o ?? -1;
  } }), r = va(n, (s, o, l) => {
    let a = ca(i, s.from);
    if (a == null)
      return;
    /\S/.test(s.text) || (a = 0);
    let f = /^\s*/.exec(s.text)[0], u = nr(n, a);
    (f != u || l.from < s.from + f.length) && (t[s.from] = a, o.push({ from: s.from, to: s.from + f.length, insert: u }));
  });
  return r.changes.empty || e(n.update(r, { userEvent: "indent" })), !0;
}, Ud = ({ state: n, dispatch: e }) => n.readOnly ? !1 : (e(n.update(va(n, (t, i) => {
  i.push({ from: t.from, insert: n.facet(ir) });
}), { userEvent: "input.indent" })), !0), Xd = ({ state: n, dispatch: e }) => n.readOnly ? !1 : (e(n.update(va(n, (t, i) => {
  let r = /^\s*/.exec(t.text)[0];
  if (!r)
    return;
  let s = Mn(r, n.tabSize), o = 0, l = nr(n, Math.max(0, s - ms(n)));
  for (; o < r.length && o < l.length && r.charCodeAt(o) == l.charCodeAt(o); )
    o++;
  i.push({ from: t.from + o, to: t.from + r.length, insert: l.slice(o) });
}), { userEvent: "delete.dedent" })), !0), tx = (n) => (n.setTabFocusMode(), !0), ix = [
  { key: "Ctrl-b", run: ma, shift: Dd, preventDefault: !0 },
  { key: "Ctrl-f", run: wd, shift: Bd },
  { key: "Ctrl-p", run: Od, shift: Nd },
  { key: "Ctrl-n", run: Ad, shift: Fd },
  { key: "Ctrl-a", run: wb, shift: Eb },
  { key: "Ctrl-e", run: Sb, shift: Ib },
  { key: "Ctrl-d", run: Hd },
  { key: "Ctrl-h", run: Pl },
  { key: "Ctrl-k", run: $b },
  { key: "Ctrl-Alt-h", run: $d },
  { key: "Ctrl-o", run: _b },
  { key: "Ctrl-t", run: jb },
  { key: "Ctrl-v", run: Tl }
], nx = /* @__PURE__ */ [
  { key: "ArrowLeft", run: ma, shift: Dd, preventDefault: !0 },
  { key: "Mod-ArrowLeft", mac: "Alt-ArrowLeft", run: gb, shift: Ab, preventDefault: !0 },
  { mac: "Cmd-ArrowLeft", run: xb, shift: Db, preventDefault: !0 },
  { key: "ArrowRight", run: wd, shift: Bd, preventDefault: !0 },
  { key: "Mod-ArrowRight", mac: "Alt-ArrowRight", run: mb, shift: Mb, preventDefault: !0 },
  { mac: "Cmd-ArrowRight", run: kb, shift: Bb, preventDefault: !0 },
  { key: "ArrowUp", run: Od, shift: Nd, preventDefault: !0 },
  { mac: "Cmd-ArrowUp", run: Dc, shift: Ec },
  { mac: "Ctrl-ArrowUp", run: Pc, shift: Rc },
  { key: "ArrowDown", run: Ad, shift: Fd, preventDefault: !0 },
  { mac: "Cmd-ArrowDown", run: Bc, shift: Ic },
  { mac: "Ctrl-ArrowDown", run: Tl, shift: Lc },
  { key: "PageUp", run: Pc, shift: Rc },
  { key: "PageDown", run: Tl, shift: Lc },
  { key: "Home", run: Rd, shift: Lb, preventDefault: !0 },
  { key: "Mod-Home", run: Dc, shift: Ec },
  { key: "End", run: Pd, shift: Rb, preventDefault: !0 },
  { key: "Mod-End", run: Bc, shift: Ic },
  { key: "Enter", run: Rl, shift: Rl },
  { key: "Mod-a", run: Nb },
  { key: "Backspace", run: Pl, shift: Pl, preventDefault: !0 },
  { key: "Delete", run: Hd, preventDefault: !0 },
  { key: "Mod-Backspace", mac: "Alt-Backspace", run: $d, preventDefault: !0 },
  { key: "Mod-Delete", mac: "Alt-Delete", run: zb, preventDefault: !0 },
  { mac: "Mod-Backspace", run: qb, preventDefault: !0 },
  { mac: "Mod-Delete", run: Kb, preventDefault: !0 }
].concat(/* @__PURE__ */ ix.map((n) => ({ mac: n.key, run: n.run, shift: n.shift }))), rx = /* @__PURE__ */ [
  { key: "Alt-ArrowLeft", mac: "Ctrl-ArrowLeft", run: yb, shift: Tb },
  { key: "Alt-ArrowRight", mac: "Ctrl-ArrowRight", run: bb, shift: Pb },
  { key: "Alt-ArrowUp", run: Ub },
  { key: "Shift-Alt-ArrowUp", run: Yb },
  { key: "Alt-ArrowDown", run: Xb },
  { key: "Shift-Alt-ArrowDown", run: Gb },
  { key: "Mod-Alt-ArrowUp", run: Qb },
  { key: "Mod-Alt-ArrowDown", run: Vb },
  { key: "Escape", run: Hb },
  { key: "Mod-Enter", run: ex },
  { key: "Alt-l", mac: "Ctrl-l", run: Fb },
  { key: "Mod-i", run: Wb, preventDefault: !0 },
  { key: "Mod-[", run: Xd },
  { key: "Mod-]", run: Ud },
  { key: "Mod-Alt-\\", run: jd },
  { key: "Shift-Mod-k", run: Zb },
  { key: "Shift-Mod-\\", run: Ob },
  { key: "Mod-/", run: Xy },
  { key: "Alt-A", run: Gy },
  { key: "Ctrl-m", mac: "Shift-Alt-m", run: tx }
].concat(nx), Nc = typeof String.prototype.normalize == "function" ? (n) => n.normalize("NFKD") : (n) => n;
class Cn {
  /**
  Create a text cursor. The query is the search string, `from` to
  `to` provides the region to search.
  
  When `normalize` is given, it will be called, on both the query
  string and the content it is matched against, before comparing.
  You can, for example, create a case-insensitive search by
  passing `s => s.toLowerCase()`.
  
  Text is always normalized with
  [`.normalize("NFKD")`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/normalize)
  (when supported).
  */
  constructor(e, t, i = 0, r = e.length, s, o) {
    this.test = o, this.value = { from: 0, to: 0 }, this.done = !1, this.matches = [], this.buffer = "", this.bufferPos = 0, this.iter = e.iterRange(i, r), this.bufferStart = i, this.normalize = s ? (l) => s(Nc(l)) : Nc, this.query = this.normalize(t);
  }
  peek() {
    if (this.bufferPos == this.buffer.length) {
      if (this.bufferStart += this.buffer.length, this.iter.next(), this.iter.done)
        return -1;
      this.bufferPos = 0, this.buffer = this.iter.value;
    }
    return at(this.buffer, this.bufferPos);
  }
  /**
  Look for the next match. Updates the iterator's
  [`value`](https://codemirror.net/6/docs/ref/#search.SearchCursor.value) and
  [`done`](https://codemirror.net/6/docs/ref/#search.SearchCursor.done) properties. Should be called
  at least once before using the cursor.
  */
  next() {
    for (; this.matches.length; )
      this.matches.pop();
    return this.nextOverlapping();
  }
  /**
  The `next` method will ignore matches that partially overlap a
  previous match. This method behaves like `next`, but includes
  such matches.
  */
  nextOverlapping() {
    for (; ; ) {
      let e = this.peek();
      if (e < 0)
        return this.done = !0, this;
      let t = Ql(e), i = this.bufferStart + this.bufferPos;
      this.bufferPos += Yt(e);
      let r = this.normalize(t);
      if (r.length)
        for (let s = 0, o = i; ; s++) {
          let l = r.charCodeAt(s), a = this.match(l, o, this.bufferPos + this.bufferStart);
          if (s == r.length - 1) {
            if (a)
              return this.value = a, this;
            break;
          }
          o == i && s < t.length && t.charCodeAt(s) == l && o++;
        }
    }
  }
  match(e, t, i) {
    let r = null;
    for (let s = 0; s < this.matches.length; s += 2) {
      let o = this.matches[s], l = !1;
      this.query.charCodeAt(o) == e && (o == this.query.length - 1 ? r = { from: this.matches[s + 1], to: i } : (this.matches[s]++, l = !0)), l || (this.matches.splice(s, 2), s -= 2);
    }
    return this.query.charCodeAt(0) == e && (this.query.length == 1 ? r = { from: t, to: i } : this.matches.push(1, t)), r && this.test && !this.test(r.from, r.to, this.buffer, this.bufferStart) && (r = null), r;
  }
}
typeof Symbol < "u" && (Cn.prototype[Symbol.iterator] = function() {
  return this;
});
const Yd = { from: -1, to: -1, match: /* @__PURE__ */ /.*/.exec("") }, ya = "gm" + (/x/.unicode == null ? "" : "u");
class ba {
  /**
  Create a cursor that will search the given range in the given
  document. `query` should be the raw pattern (as you'd pass it to
  `new RegExp`).
  */
  constructor(e, t, i, r = 0, s = e.length) {
    if (this.text = e, this.to = s, this.curLine = "", this.done = !1, this.value = Yd, /\\[sWDnr]|\n|\r|\[\^/.test(t))
      return new Gd(e, t, i, r, s);
    this.re = new RegExp(t, ya + (i?.ignoreCase ? "i" : "")), this.test = i?.test, this.iter = e.iter();
    let o = e.lineAt(r);
    this.curLineStart = o.from, this.matchPos = ks(e, r), this.getLine(this.curLineStart);
  }
  getLine(e) {
    this.iter.next(e), this.iter.lineBreak ? this.curLine = "" : (this.curLine = this.iter.value, this.curLineStart + this.curLine.length > this.to && (this.curLine = this.curLine.slice(0, this.to - this.curLineStart)), this.iter.next());
  }
  nextLine() {
    this.curLineStart = this.curLineStart + this.curLine.length + 1, this.curLineStart > this.to ? this.curLine = "" : this.getLine(0);
  }
  /**
  Move to the next match, if there is one.
  */
  next() {
    for (let e = this.matchPos - this.curLineStart; ; ) {
      this.re.lastIndex = e;
      let t = this.matchPos <= this.to && this.re.exec(this.curLine);
      if (t) {
        let i = this.curLineStart + t.index, r = i + t[0].length;
        if (this.matchPos = ks(this.text, r + (i == r ? 1 : 0)), i == this.curLineStart + this.curLine.length && this.nextLine(), (i < r || i > this.value.to) && (!this.test || this.test(i, r, t)))
          return this.value = { from: i, to: r, match: t }, this;
        e = this.matchPos - this.curLineStart;
      } else if (this.curLineStart + this.curLine.length < this.to)
        this.nextLine(), e = 0;
      else
        return this.done = !0, this;
    }
  }
}
const Ro = /* @__PURE__ */ new WeakMap();
class pn {
  constructor(e, t) {
    this.from = e, this.text = t;
  }
  get to() {
    return this.from + this.text.length;
  }
  static get(e, t, i) {
    let r = Ro.get(e);
    if (!r || r.from >= i || r.to <= t) {
      let l = new pn(t, e.sliceString(t, i));
      return Ro.set(e, l), l;
    }
    if (r.from == t && r.to == i)
      return r;
    let { text: s, from: o } = r;
    return o > t && (s = e.sliceString(t, o) + s, o = t), r.to < i && (s += e.sliceString(r.to, i)), Ro.set(e, new pn(o, s)), new pn(t, s.slice(t - o, i - o));
  }
}
class Gd {
  constructor(e, t, i, r, s) {
    this.text = e, this.to = s, this.done = !1, this.value = Yd, this.matchPos = ks(e, r), this.re = new RegExp(t, ya + (i?.ignoreCase ? "i" : "")), this.test = i?.test, this.flat = pn.get(e, r, this.chunkEnd(
      r + 5e3
      /* Chunk.Base */
    ));
  }
  chunkEnd(e) {
    return e >= this.to ? this.to : this.text.lineAt(e).to;
  }
  next() {
    for (; ; ) {
      let e = this.re.lastIndex = this.matchPos - this.flat.from, t = this.re.exec(this.flat.text);
      if (t && !t[0] && t.index == e && (this.re.lastIndex = e + 1, t = this.re.exec(this.flat.text)), t) {
        let i = this.flat.from + t.index, r = i + t[0].length;
        if ((this.flat.to >= this.to || t.index + t[0].length <= this.flat.text.length - 10) && (!this.test || this.test(i, r, t)))
          return this.value = { from: i, to: r, match: t }, this.matchPos = ks(this.text, r + (i == r ? 1 : 0)), this;
      }
      if (this.flat.to == this.to)
        return this.done = !0, this;
      this.flat = pn.get(this.text, this.flat.from, this.chunkEnd(this.flat.from + this.flat.text.length * 2));
    }
  }
}
typeof Symbol < "u" && (ba.prototype[Symbol.iterator] = Gd.prototype[Symbol.iterator] = function() {
  return this;
});
function sx(n) {
  try {
    return new RegExp(n, ya), !0;
  } catch {
    return !1;
  }
}
function ks(n, e) {
  if (e >= n.length)
    return e;
  let t = n.lineAt(e), i;
  for (; e < t.to && (i = t.text.charCodeAt(e - t.from)) >= 56320 && i < 57344; )
    e++;
  return e;
}
const ox = (n) => {
  let { state: e } = n, t = String(e.doc.lineAt(n.state.selection.main.head).number), { close: i, result: r } = Sv(n, {
    label: e.phrase("Go to line"),
    input: { type: "text", name: "line", value: t },
    focus: !0,
    submitLabel: e.phrase("go")
  });
  return r.then((s) => {
    let o = s && /^([+-])?(\d+)?(:\d+)?(%)?$/.exec(s.elements.line.value);
    if (!o) {
      n.dispatch({ effects: i });
      return;
    }
    let l = e.doc.lineAt(e.selection.main.head), [, a, f, u, g] = o, y = u ? +u.slice(1) : 0, b = f ? +f : l.number;
    if (f && g) {
      let M = b / 100;
      a && (M = M * (a == "-" ? -1 : 1) + l.number / e.doc.lines), b = Math.round(e.doc.lines * M);
    } else f && a && (b = b * (a == "-" ? -1 : 1) + l.number);
    let k = e.doc.line(Math.max(1, Math.min(e.doc.lines, b))), C = E.cursor(k.from + Math.max(0, Math.min(y, k.length)));
    n.dispatch({
      effects: [i, _.scrollIntoView(C.from, { y: "center" })],
      selection: C
    });
  }), !0;
}, lx = {
  highlightWordAroundCursor: !1,
  minSelectionLength: 1,
  maxMatches: 100,
  wholeWords: !1
}, ax = /* @__PURE__ */ U.define({
  combine(n) {
    return ti(n, lx, {
      highlightWordAroundCursor: (e, t) => e || t,
      minSelectionLength: Math.min,
      maxMatches: Math.min
    });
  }
});
function hx(n) {
  return [px, dx];
}
const cx = /* @__PURE__ */ G.mark({ class: "cm-selectionMatch" }), fx = /* @__PURE__ */ G.mark({ class: "cm-selectionMatch cm-selectionMatch-main" });
function Fc(n, e, t, i) {
  return (t == 0 || n(e.sliceDoc(t - 1, t)) != Me.Word) && (i == e.doc.length || n(e.sliceDoc(i, i + 1)) != Me.Word);
}
function ux(n, e, t, i) {
  return n(e.sliceDoc(t, t + 1)) == Me.Word && n(e.sliceDoc(i - 1, i)) == Me.Word;
}
const dx = /* @__PURE__ */ De.fromClass(class {
  constructor(n) {
    this.decorations = this.getDeco(n);
  }
  update(n) {
    (n.selectionSet || n.docChanged || n.viewportChanged) && (this.decorations = this.getDeco(n.view));
  }
  getDeco(n) {
    let e = n.state.facet(ax), { state: t } = n, i = t.selection;
    if (i.ranges.length > 1)
      return G.none;
    let r = i.main, s, o = null;
    if (r.empty) {
      if (!e.highlightWordAroundCursor)
        return G.none;
      let a = t.wordAt(r.head);
      if (!a)
        return G.none;
      o = t.charCategorizer(r.head), s = t.sliceDoc(a.from, a.to);
    } else {
      let a = r.to - r.from;
      if (a < e.minSelectionLength || a > 200)
        return G.none;
      if (e.wholeWords) {
        if (s = t.sliceDoc(r.from, r.to), o = t.charCategorizer(r.head), !(Fc(o, t, r.from, r.to) && ux(o, t, r.from, r.to)))
          return G.none;
      } else if (s = t.sliceDoc(r.from, r.to), !s)
        return G.none;
    }
    let l = [];
    for (let a of n.visibleRanges) {
      let f = new Cn(t.doc, s, a.from, a.to);
      for (; !f.next().done; ) {
        let { from: u, to: g } = f.value;
        if ((!o || Fc(o, t, u, g)) && (r.empty && u <= r.from && g >= r.to ? l.push(fx.range(u, g)) : (u >= r.to || g <= r.from) && l.push(cx.range(u, g)), l.length > e.maxMatches))
          return G.none;
      }
    }
    return G.set(l);
  }
}, {
  decorations: (n) => n.decorations
}), px = /* @__PURE__ */ _.baseTheme({
  ".cm-selectionMatch": { backgroundColor: "#99ff7780" },
  ".cm-searchMatch .cm-selectionMatch": { backgroundColor: "transparent" }
}), gx = ({ state: n, dispatch: e }) => {
  let { selection: t } = n, i = E.create(t.ranges.map((r) => n.wordAt(r.head) || E.cursor(r.head)), t.mainIndex);
  return i.eq(t) ? !1 : (e(n.update({ selection: i })), !0);
};
function mx(n, e) {
  let { main: t, ranges: i } = n.selection, r = n.wordAt(t.head), s = r && r.from == t.from && r.to == t.to;
  for (let o = !1, l = new Cn(n.doc, e, i[i.length - 1].to); ; )
    if (l.next(), l.done) {
      if (o)
        return null;
      l = new Cn(n.doc, e, 0, Math.max(0, i[i.length - 1].from - 1)), o = !0;
    } else {
      if (o && i.some((a) => a.from == l.value.from))
        continue;
      if (s) {
        let a = n.wordAt(l.value.from);
        if (!a || a.from != l.value.from || a.to != l.value.to)
          continue;
      }
      return l.value;
    }
}
const vx = ({ state: n, dispatch: e }) => {
  let { ranges: t } = n.selection;
  if (t.some((s) => s.from === s.to))
    return gx({ state: n, dispatch: e });
  let i = n.sliceDoc(t[0].from, t[0].to);
  if (n.selection.ranges.some((s) => n.sliceDoc(s.from, s.to) != i))
    return !1;
  let r = mx(n, i);
  return r ? (e(n.update({
    selection: n.selection.addRange(E.range(r.from, r.to), !1),
    effects: _.scrollIntoView(r.to)
  })), !0) : !1;
}, Pn = /* @__PURE__ */ U.define({
  combine(n) {
    return ti(n, {
      top: !1,
      caseSensitive: !1,
      literal: !1,
      regexp: !1,
      wholeWord: !1,
      createPanel: (e) => new Lx(e),
      scrollToMatch: (e) => _.scrollIntoView(e)
    });
  }
});
class xa {
  /**
  Create a query object.
  */
  constructor(e) {
    this.search = e.search, this.caseSensitive = !!e.caseSensitive, this.literal = !!e.literal, this.regexp = !!e.regexp, this.replace = e.replace || "", this.valid = !!this.search && (!this.regexp || sx(this.search)), this.unquoted = this.unquote(this.search), this.wholeWord = !!e.wholeWord, this.test = e.test;
  }
  /**
  @internal
  */
  unquote(e) {
    return this.literal ? e : e.replace(/\\([nrt\\])/g, (t, i) => i == "n" ? `
` : i == "r" ? "\r" : i == "t" ? "	" : "\\");
  }
  /**
  Compare this query to another query.
  */
  eq(e) {
    return this.search == e.search && this.replace == e.replace && this.caseSensitive == e.caseSensitive && this.regexp == e.regexp && this.wholeWord == e.wholeWord && this.test == e.test;
  }
  /**
  @internal
  */
  create() {
    return this.regexp ? new Sx(this) : new xx(this);
  }
  /**
  Get a search cursor for this query, searching through the given
  range in the given state.
  */
  getCursor(e, t = 0, i) {
    let r = e.doc ? e : pe.create({ doc: e });
    return i == null && (i = r.doc.length), this.regexp ? rn(this, r, t, i) : nn(this, r, t, i);
  }
}
class Zd {
  constructor(e) {
    this.spec = e;
  }
}
function yx(n, e, t) {
  return (i, r, s, o) => {
    if (t && !t(i, r, s, o))
      return !1;
    let l = i >= o && r <= o + s.length ? s.slice(i - o, r - o) : e.doc.sliceString(i, r);
    return n(l, e, i, r);
  };
}
function nn(n, e, t, i) {
  let r;
  return n.wholeWord && (r = bx(e.doc, e.charCategorizer(e.selection.main.head))), n.test && (r = yx(n.test, e, r)), new Cn(e.doc, n.unquoted, t, i, n.caseSensitive ? void 0 : (s) => s.toLowerCase(), r);
}
function bx(n, e) {
  return (t, i, r, s) => ((s > t || s + r.length < i) && (s = Math.max(0, t - 2), r = n.sliceString(s, Math.min(n.length, i + 2))), (e(ws(r, t - s)) != Me.Word || e(Ss(r, t - s)) != Me.Word) && (e(Ss(r, i - s)) != Me.Word || e(ws(r, i - s)) != Me.Word));
}
class xx extends Zd {
  constructor(e) {
    super(e);
  }
  nextMatch(e, t, i) {
    let r = nn(this.spec, e, i, e.doc.length).nextOverlapping();
    if (r.done) {
      let s = Math.min(e.doc.length, t + this.spec.unquoted.length);
      r = nn(this.spec, e, 0, s).nextOverlapping();
    }
    return r.done || r.value.from == t && r.value.to == i ? null : r.value;
  }
  // Searching in reverse is, rather than implementing an inverted search
  // cursor, done by scanning chunk after chunk forward.
  prevMatchInRange(e, t, i) {
    for (let r = i; ; ) {
      let s = Math.max(t, r - 1e4 - this.spec.unquoted.length), o = nn(this.spec, e, s, r), l = null;
      for (; !o.nextOverlapping().done; )
        l = o.value;
      if (l)
        return l;
      if (s == t)
        return null;
      r -= 1e4;
    }
  }
  prevMatch(e, t, i) {
    let r = this.prevMatchInRange(e, 0, t);
    return r || (r = this.prevMatchInRange(e, Math.max(0, i - this.spec.unquoted.length), e.doc.length)), r && (r.from != t || r.to != i) ? r : null;
  }
  getReplacement(e) {
    return this.spec.unquote(this.spec.replace);
  }
  matchAll(e, t) {
    let i = nn(this.spec, e, 0, e.doc.length), r = [];
    for (; !i.next().done; ) {
      if (r.length >= t)
        return null;
      r.push(i.value);
    }
    return r;
  }
  highlight(e, t, i, r) {
    let s = nn(this.spec, e, Math.max(0, t - this.spec.unquoted.length), Math.min(i + this.spec.unquoted.length, e.doc.length));
    for (; !s.next().done; )
      r(s.value.from, s.value.to);
  }
}
function kx(n, e, t) {
  return (i, r, s) => (!t || t(i, r, s)) && n(s[0], e, i, r);
}
function rn(n, e, t, i) {
  let r;
  return n.wholeWord && (r = wx(e.charCategorizer(e.selection.main.head))), n.test && (r = kx(n.test, e, r)), new ba(e.doc, n.search, { ignoreCase: !n.caseSensitive, test: r }, t, i);
}
function ws(n, e) {
  return n.slice(We(n, e, !1), e);
}
function Ss(n, e) {
  return n.slice(e, We(n, e));
}
function wx(n) {
  return (e, t, i) => !i[0].length || (n(ws(i.input, i.index)) != Me.Word || n(Ss(i.input, i.index)) != Me.Word) && (n(Ss(i.input, i.index + i[0].length)) != Me.Word || n(ws(i.input, i.index + i[0].length)) != Me.Word);
}
class Sx extends Zd {
  nextMatch(e, t, i) {
    let r = rn(this.spec, e, i, e.doc.length).next();
    return r.done && (r = rn(this.spec, e, 0, t).next()), r.done ? null : r.value;
  }
  prevMatchInRange(e, t, i) {
    for (let r = 1; ; r++) {
      let s = Math.max(
        t,
        i - r * 1e4
        /* FindPrev.ChunkSize */
      ), o = rn(this.spec, e, s, i), l = null;
      for (; !o.next().done; )
        l = o.value;
      if (l && (s == t || l.from > s + 10))
        return l;
      if (s == t)
        return null;
    }
  }
  prevMatch(e, t, i) {
    return this.prevMatchInRange(e, 0, t) || this.prevMatchInRange(e, i, e.doc.length);
  }
  getReplacement(e) {
    return this.spec.unquote(this.spec.replace).replace(/\$([$&]|\d+)/g, (t, i) => {
      if (i == "&")
        return e.match[0];
      if (i == "$")
        return "$";
      for (let r = i.length; r > 0; r--) {
        let s = +i.slice(0, r);
        if (s > 0 && s < e.match.length)
          return e.match[s] + i.slice(r);
      }
      return t;
    });
  }
  matchAll(e, t) {
    let i = rn(this.spec, e, 0, e.doc.length), r = [];
    for (; !i.next().done; ) {
      if (r.length >= t)
        return null;
      r.push(i.value);
    }
    return r;
  }
  highlight(e, t, i, r) {
    let s = rn(this.spec, e, Math.max(
      0,
      t - 250
      /* RegExp.HighlightMargin */
    ), Math.min(i + 250, e.doc.length));
    for (; !s.next().done; )
      r(s.value.from, s.value.to);
  }
}
const Mi = /* @__PURE__ */ ne.define(), ka = /* @__PURE__ */ ne.define(), xi = /* @__PURE__ */ $e.define({
  create(n) {
    return new Lo(Ll(n).create(), null);
  },
  update(n, e) {
    for (let t of e.effects)
      t.is(Mi) ? n = new Lo(t.value.create(), n.panel) : t.is(ka) && (n = new Lo(n.query, t.value ? wa : null));
    return n;
  },
  provide: (n) => ji.from(n, (e) => e.panel)
});
class Lo {
  constructor(e, t) {
    this.query = e, this.panel = t;
  }
}
const Cx = /* @__PURE__ */ G.mark({ class: "cm-searchMatch" }), Ox = /* @__PURE__ */ G.mark({ class: "cm-searchMatch cm-searchMatch-selected" }), Ax = /* @__PURE__ */ De.fromClass(class {
  constructor(n) {
    this.view = n, this.decorations = this.highlight(n.state.field(xi));
  }
  update(n) {
    let e = n.state.field(xi);
    (e != n.startState.field(xi) || n.docChanged || n.selectionSet || n.viewportChanged) && (this.decorations = this.highlight(e));
  }
  highlight({ query: n, panel: e }) {
    if (!e || !n.spec.valid)
      return G.none;
    let { view: t } = this, i = new ei();
    for (let r = 0, s = t.visibleRanges, o = s.length; r < o; r++) {
      let { from: l, to: a } = s[r];
      for (; r < o - 1 && a > s[r + 1].from - 500; )
        a = s[++r].to;
      n.highlight(t.state, l, a, (f, u) => {
        let g = t.state.selection.ranges.some((y) => y.from == f && y.to == u);
        i.add(f, u, g ? Ox : Cx);
      });
    }
    return i.finish();
  }
}, {
  decorations: (n) => n.decorations
});
function vr(n) {
  return (e) => {
    let t = e.state.field(xi, !1);
    return t && t.query.spec.valid ? n(e, t) : tp(e);
  };
}
const Cs = /* @__PURE__ */ vr((n, { query: e }) => {
  let { to: t } = n.state.selection.main, i = e.nextMatch(n.state, t, t);
  if (!i)
    return !1;
  let r = E.single(i.from, i.to), s = n.state.facet(Pn);
  return n.dispatch({
    selection: r,
    effects: [Sa(n, i), s.scrollToMatch(r.main, n)],
    userEvent: "select.search"
  }), ep(n), !0;
}), Os = /* @__PURE__ */ vr((n, { query: e }) => {
  let { state: t } = n, { from: i } = t.selection.main, r = e.prevMatch(t, i, i);
  if (!r)
    return !1;
  let s = E.single(r.from, r.to), o = n.state.facet(Pn);
  return n.dispatch({
    selection: s,
    effects: [Sa(n, r), o.scrollToMatch(s.main, n)],
    userEvent: "select.search"
  }), ep(n), !0;
}), Mx = /* @__PURE__ */ vr((n, { query: e }) => {
  let t = e.matchAll(n.state, 1e3);
  return !t || !t.length ? !1 : (n.dispatch({
    selection: E.create(t.map((i) => E.range(i.from, i.to))),
    userEvent: "select.search.matches"
  }), !0);
}), Tx = ({ state: n, dispatch: e }) => {
  let t = n.selection;
  if (t.ranges.length > 1 || t.main.empty)
    return !1;
  let { from: i, to: r } = t.main, s = [], o = 0;
  for (let l = new Cn(n.doc, n.sliceDoc(i, r)); !l.next().done; ) {
    if (s.length > 1e3)
      return !1;
    l.value.from == i && (o = s.length), s.push(E.range(l.value.from, l.value.to));
  }
  return e(n.update({
    selection: E.create(s, o),
    userEvent: "select.search.matches"
  })), !0;
}, Wc = /* @__PURE__ */ vr((n, { query: e }) => {
  let { state: t } = n, { from: i, to: r } = t.selection.main;
  if (t.readOnly)
    return !1;
  let s = e.nextMatch(t, i, i);
  if (!s)
    return !1;
  let o = s, l = [], a, f, u = [];
  o.from == i && o.to == r && (f = t.toText(e.getReplacement(o)), l.push({ from: o.from, to: o.to, insert: f }), o = e.nextMatch(t, o.from, o.to), u.push(_.announce.of(t.phrase("replaced match on line $", t.doc.lineAt(i).number) + ".")));
  let g = n.state.changes(l);
  return o && (a = E.single(o.from, o.to).map(g), u.push(Sa(n, o)), u.push(t.facet(Pn).scrollToMatch(a.main, n))), n.dispatch({
    changes: g,
    selection: a,
    effects: u,
    userEvent: "input.replace"
  }), !0;
}), Px = /* @__PURE__ */ vr((n, { query: e }) => {
  if (n.state.readOnly)
    return !1;
  let t = e.matchAll(n.state, 1e9).map((r) => {
    let { from: s, to: o } = r;
    return { from: s, to: o, insert: e.getReplacement(r) };
  });
  if (!t.length)
    return !1;
  let i = n.state.phrase("replaced $ matches", t.length) + ".";
  return n.dispatch({
    changes: t,
    effects: _.announce.of(i),
    userEvent: "input.replace.all"
  }), !0;
});
function wa(n) {
  return n.state.facet(Pn).createPanel(n);
}
function Ll(n, e) {
  var t, i, r, s, o;
  let l = n.selection.main, a = l.empty || l.to > l.from + 100 ? "" : n.sliceDoc(l.from, l.to);
  if (e && !a)
    return e;
  let f = n.facet(Pn);
  return new xa({
    search: ((t = e?.literal) !== null && t !== void 0 ? t : f.literal) ? a : a.replace(/\n/g, "\\n"),
    caseSensitive: (i = e?.caseSensitive) !== null && i !== void 0 ? i : f.caseSensitive,
    literal: (r = e?.literal) !== null && r !== void 0 ? r : f.literal,
    regexp: (s = e?.regexp) !== null && s !== void 0 ? s : f.regexp,
    wholeWord: (o = e?.wholeWord) !== null && o !== void 0 ? o : f.wholeWord
  });
}
function Jd(n) {
  let e = ra(n, wa);
  return e && e.dom.querySelector("[main-field]");
}
function ep(n) {
  let e = Jd(n);
  e && e == n.root.activeElement && e.select();
}
const tp = (n) => {
  let e = n.state.field(xi, !1);
  if (e && e.panel) {
    let t = Jd(n);
    if (t && t != n.root.activeElement) {
      let i = Ll(n.state, e.query.spec);
      i.valid && n.dispatch({ effects: Mi.of(i) }), t.focus(), t.select();
    }
  } else
    n.dispatch({ effects: [
      ka.of(!0),
      e ? Mi.of(Ll(n.state, e.query.spec)) : ne.appendConfig.of(Bx)
    ] });
  return !0;
}, ip = (n) => {
  let e = n.state.field(xi, !1);
  if (!e || !e.panel)
    return !1;
  let t = ra(n, wa);
  return t && t.dom.contains(n.root.activeElement) && n.focus(), n.dispatch({ effects: ka.of(!1) }), !0;
}, Rx = [
  { key: "Mod-f", run: tp, scope: "editor search-panel" },
  { key: "F3", run: Cs, shift: Os, scope: "editor search-panel", preventDefault: !0 },
  { key: "Mod-g", run: Cs, shift: Os, scope: "editor search-panel", preventDefault: !0 },
  { key: "Escape", run: ip, scope: "editor search-panel" },
  { key: "Mod-Shift-l", run: Tx },
  { key: "Mod-Alt-g", run: ox },
  { key: "Mod-d", run: vx, preventDefault: !0 }
];
class Lx {
  constructor(e) {
    this.view = e;
    let t = this.query = e.state.field(xi).query.spec;
    this.commit = this.commit.bind(this), this.searchField = ye("input", {
      value: t.search,
      placeholder: pt(e, "Find"),
      "aria-label": pt(e, "Find"),
      class: "cm-textfield",
      name: "search",
      form: "",
      "main-field": "true",
      onchange: this.commit,
      onkeyup: this.commit
    }), this.replaceField = ye("input", {
      value: t.replace,
      placeholder: pt(e, "Replace"),
      "aria-label": pt(e, "Replace"),
      class: "cm-textfield",
      name: "replace",
      form: "",
      onchange: this.commit,
      onkeyup: this.commit
    }), this.caseField = ye("input", {
      type: "checkbox",
      name: "case",
      form: "",
      checked: t.caseSensitive,
      onchange: this.commit
    }), this.reField = ye("input", {
      type: "checkbox",
      name: "re",
      form: "",
      checked: t.regexp,
      onchange: this.commit
    }), this.wordField = ye("input", {
      type: "checkbox",
      name: "word",
      form: "",
      checked: t.wholeWord,
      onchange: this.commit
    });
    function i(r, s, o) {
      return ye("button", { class: "cm-button", name: r, onclick: s, type: "button" }, o);
    }
    this.dom = ye("div", { onkeydown: (r) => this.keydown(r), class: "cm-search" }, [
      this.searchField,
      i("next", () => Cs(e), [pt(e, "next")]),
      i("prev", () => Os(e), [pt(e, "previous")]),
      i("select", () => Mx(e), [pt(e, "all")]),
      ye("label", null, [this.caseField, pt(e, "match case")]),
      ye("label", null, [this.reField, pt(e, "regexp")]),
      ye("label", null, [this.wordField, pt(e, "by word")]),
      ...e.state.readOnly ? [] : [
        ye("br"),
        this.replaceField,
        i("replace", () => Wc(e), [pt(e, "replace")]),
        i("replaceAll", () => Px(e), [pt(e, "replace all")])
      ],
      ye("button", {
        name: "close",
        onclick: () => ip(e),
        "aria-label": pt(e, "close"),
        type: "button"
      }, ["×"])
    ]);
  }
  commit() {
    let e = new xa({
      search: this.searchField.value,
      caseSensitive: this.caseField.checked,
      regexp: this.reField.checked,
      wholeWord: this.wordField.checked,
      replace: this.replaceField.value
    });
    e.eq(this.query) || (this.query = e, this.view.dispatch({ effects: Mi.of(e) }));
  }
  keydown(e) {
    Ni(this.view, e, "search-panel") ? e.preventDefault() : e.keyCode == 13 && e.target == this.searchField ? (e.preventDefault(), (e.shiftKey ? Os : Cs)(this.view)) : e.keyCode == 13 && e.target == this.replaceField && (e.preventDefault(), Wc(this.view));
  }
  update(e) {
    for (let t of e.transactions)
      for (let i of t.effects)
        i.is(Mi) && !i.value.eq(this.query) && this.setQuery(i.value);
  }
  setQuery(e) {
    this.query = e, this.searchField.value = e.search, this.replaceField.value = e.replace, this.caseField.checked = e.caseSensitive, this.reField.checked = e.regexp, this.wordField.checked = e.wholeWord;
  }
  mount() {
    this.searchField.select();
  }
  get pos() {
    return 80;
  }
  get top() {
    return this.view.state.facet(Pn).top;
  }
}
function pt(n, e) {
  return n.state.phrase(e);
}
const Vr = 30, Hr = /[\s\.,:;?!]/;
function Sa(n, { from: e, to: t }) {
  let i = n.state.doc.lineAt(e), r = n.state.doc.lineAt(t).to, s = Math.max(i.from, e - Vr), o = Math.min(r, t + Vr), l = n.state.sliceDoc(s, o);
  if (s != i.from) {
    for (let a = 0; a < Vr; a++)
      if (!Hr.test(l[a + 1]) && Hr.test(l[a])) {
        l = l.slice(a);
        break;
      }
  }
  if (o != r) {
    for (let a = l.length - 1; a > l.length - Vr; a--)
      if (!Hr.test(l[a - 1]) && Hr.test(l[a])) {
        l = l.slice(0, a);
        break;
      }
  }
  return _.announce.of(`${n.state.phrase("current match")}. ${l} ${n.state.phrase("on line")} ${i.number}.`);
}
const Dx = /* @__PURE__ */ _.baseTheme({
  ".cm-panel.cm-search": {
    padding: "2px 6px 4px",
    position: "relative",
    "& [name=close]": {
      position: "absolute",
      top: "0",
      right: "4px",
      backgroundColor: "inherit",
      border: "none",
      font: "inherit",
      padding: 0,
      margin: 0
    },
    "& input, & button, & label": {
      margin: ".2em .6em .2em 0"
    },
    "& input[type=checkbox]": {
      marginRight: ".2em"
    },
    "& label": {
      fontSize: "80%",
      whiteSpace: "pre"
    }
  },
  "&light .cm-searchMatch": { backgroundColor: "#ffff0054" },
  "&dark .cm-searchMatch": { backgroundColor: "#00ffff8a" },
  "&light .cm-searchMatch-selected": { backgroundColor: "#ff6a0054" },
  "&dark .cm-searchMatch-selected": { backgroundColor: "#ff00ff8a" }
}), Bx = [
  xi,
  /* @__PURE__ */ Ti.low(Ax),
  Dx
];
class np {
  /**
  Create a new completion context. (Mostly useful for testing
  completion sources—in the editor, the extension will create
  these for you.)
  */
  constructor(e, t, i, r) {
    this.state = e, this.pos = t, this.explicit = i, this.view = r, this.abortListeners = [], this.abortOnDocChange = !1;
  }
  /**
  Get the extent, content, and (if there is a token) type of the
  token before `this.pos`.
  */
  tokenBefore(e) {
    let t = Ve(this.state).resolveInner(this.pos, -1);
    for (; t && e.indexOf(t.name) < 0; )
      t = t.parent;
    return t ? {
      from: t.from,
      to: this.pos,
      text: this.state.sliceDoc(t.from, this.pos),
      type: t.type
    } : null;
  }
  /**
  Get the match of the given expression directly before the
  cursor.
  */
  matchBefore(e) {
    let t = this.state.doc.lineAt(this.pos), i = Math.max(t.from, this.pos - 250), r = t.text.slice(i - t.from, this.pos - t.from), s = r.search(rp(e, !1));
    return s < 0 ? null : { from: i + s, to: this.pos, text: r.slice(s) };
  }
  /**
  Yields true when the query has been aborted. Can be useful in
  asynchronous queries to avoid doing work that will be ignored.
  */
  get aborted() {
    return this.abortListeners == null;
  }
  /**
  Allows you to register abort handlers, which will be called when
  the query is
  [aborted](https://codemirror.net/6/docs/ref/#autocomplete.CompletionContext.aborted).
  
  By default, running queries will not be aborted for regular
  typing or backspacing, on the assumption that they are likely to
  return a result with a
  [`validFor`](https://codemirror.net/6/docs/ref/#autocomplete.CompletionResult.validFor) field that
  allows the result to be used after all. Passing `onDocChange:
  true` will cause this query to be aborted for any document
  change.
  */
  addEventListener(e, t, i) {
    e == "abort" && this.abortListeners && (this.abortListeners.push(t), i && i.onDocChange && (this.abortOnDocChange = !0));
  }
}
function Qc(n) {
  let e = Object.keys(n).join(""), t = /\w/.test(e);
  return t && (e = e.replace(/\w/g, "")), `[${t ? "\\w" : ""}${e.replace(/[^\w\s]/g, "\\$&")}]`;
}
function Ex(n) {
  let e = /* @__PURE__ */ Object.create(null), t = /* @__PURE__ */ Object.create(null);
  for (let { label: r } of n) {
    e[r[0]] = !0;
    for (let s = 1; s < r.length; s++)
      t[r[s]] = !0;
  }
  let i = Qc(e) + Qc(t) + "*$";
  return [new RegExp("^" + i), new RegExp(i)];
}
function Ca(n) {
  let e = n.map((r) => typeof r == "string" ? { label: r } : r), [t, i] = e.every((r) => /^\w+$/.test(r.label)) ? [/\w*$/, /\w+$/] : Ex(e);
  return (r) => {
    let s = r.matchBefore(i);
    return s || r.explicit ? { from: s ? s.from : r.pos, options: e, validFor: t } : null;
  };
}
function Ix(n, e) {
  return (t) => {
    for (let i = Ve(t.state).resolveInner(t.pos, -1); i; i = i.parent) {
      if (n.indexOf(i.name) > -1)
        return null;
      if (i.type.isTop)
        break;
    }
    return e(t);
  };
}
class Vc {
  constructor(e, t, i, r) {
    this.completion = e, this.source = t, this.match = i, this.score = r;
  }
}
function $i(n) {
  return n.selection.main.from;
}
function rp(n, e) {
  var t;
  let { source: i } = n, r = e && i[0] != "^", s = i[i.length - 1] != "$";
  return !r && !s ? n : new RegExp(`${r ? "^" : ""}(?:${i})${s ? "$" : ""}`, (t = n.flags) !== null && t !== void 0 ? t : n.ignoreCase ? "i" : "");
}
const sp = /* @__PURE__ */ hi.define();
function Nx(n, e, t, i) {
  let { main: r } = n.selection, s = t - r.from, o = i - r.from;
  return {
    ...n.changeByRange((l) => {
      if (l != r && t != i && n.sliceDoc(l.from + s, l.from + o) != n.sliceDoc(t, i))
        return { range: l };
      let a = n.toText(e);
      return {
        changes: { from: l.from + s, to: i == r.from ? l.to : l.from + o, insert: a },
        range: E.cursor(l.from + s + a.length)
      };
    }),
    scrollIntoView: !0,
    userEvent: "input.complete"
  };
}
const Hc = /* @__PURE__ */ new WeakMap();
function Fx(n) {
  if (!Array.isArray(n))
    return n;
  let e = Hc.get(n);
  return e || Hc.set(n, e = Ca(n)), e;
}
const As = /* @__PURE__ */ ne.define(), sr = /* @__PURE__ */ ne.define();
class Wx {
  constructor(e) {
    this.pattern = e, this.chars = [], this.folded = [], this.any = [], this.precise = [], this.byWord = [], this.score = 0, this.matched = [];
    for (let t = 0; t < e.length; ) {
      let i = at(e, t), r = Yt(i);
      this.chars.push(i);
      let s = e.slice(t, t + r), o = s.toUpperCase();
      this.folded.push(at(o == s ? s.toLowerCase() : o, 0)), t += r;
    }
    this.astral = e.length != this.chars.length;
  }
  ret(e, t) {
    return this.score = e, this.matched = t, this;
  }
  // Matches a given word (completion) against the pattern (input).
  // Will return a boolean indicating whether there was a match and,
  // on success, set `this.score` to the score, `this.matched` to an
  // array of `from, to` pairs indicating the matched parts of `word`.
  //
  // The score is a number that is more negative the worse the match
  // is. See `Penalty` above.
  match(e) {
    if (this.pattern.length == 0)
      return this.ret(-100, []);
    if (e.length < this.pattern.length)
      return null;
    let { chars: t, folded: i, any: r, precise: s, byWord: o } = this;
    if (t.length == 1) {
      let I = at(e, 0), N = Yt(I), z = N == e.length ? 0 : -100;
      if (I != t[0]) if (I == i[0])
        z += -200;
      else
        return null;
      return this.ret(z, [0, N]);
    }
    let l = e.indexOf(this.pattern);
    if (l == 0)
      return this.ret(e.length == this.pattern.length ? 0 : -100, [0, this.pattern.length]);
    let a = t.length, f = 0;
    if (l < 0) {
      for (let I = 0, N = Math.min(e.length, 200); I < N && f < a; ) {
        let z = at(e, I);
        (z == t[f] || z == i[f]) && (r[f++] = I), I += Yt(z);
      }
      if (f < a)
        return null;
    }
    let u = 0, g = 0, y = !1, b = 0, k = -1, C = -1, M = /[a-z]/.test(e), R = !0;
    for (let I = 0, N = Math.min(e.length, 200), z = 0; I < N && g < a; ) {
      let F = at(e, I);
      l < 0 && (u < a && F == t[u] && (s[u++] = I), b < a && (F == t[b] || F == i[b] ? (b == 0 && (k = I), C = I + 1, b++) : b = 0));
      let H, Q = F < 255 ? F >= 48 && F <= 57 || F >= 97 && F <= 122 ? 2 : F >= 65 && F <= 90 ? 1 : 0 : (H = Ql(F)) != H.toLowerCase() ? 1 : H != H.toUpperCase() ? 2 : 0;
      (!I || Q == 1 && M || z == 0 && Q != 0) && (t[g] == F || i[g] == F && (y = !0) ? o[g++] = I : o.length && (R = !1)), z = Q, I += Yt(F);
    }
    return g == a && o[0] == 0 && R ? this.result(-100 + (y ? -200 : 0), o, e) : b == a && k == 0 ? this.ret(-200 - e.length + (C == e.length ? 0 : -100), [0, C]) : l > -1 ? this.ret(-700 - e.length, [l, l + this.pattern.length]) : b == a ? this.ret(-900 - e.length, [k, C]) : g == a ? this.result(-100 + (y ? -200 : 0) + -700 + (R ? 0 : -1100), o, e) : t.length == 2 ? null : this.result((r[0] ? -700 : 0) + -200 + -1100, r, e);
  }
  result(e, t, i) {
    let r = [], s = 0;
    for (let o of t) {
      let l = o + (this.astral ? Yt(at(i, o)) : 1);
      s && r[s - 1] == o ? r[s - 1] = l : (r[s++] = o, r[s++] = l);
    }
    return this.ret(e - i.length, r);
  }
}
class Qx {
  constructor(e) {
    this.pattern = e, this.matched = [], this.score = 0, this.folded = e.toLowerCase();
  }
  match(e) {
    if (e.length < this.pattern.length)
      return null;
    let t = e.slice(0, this.pattern.length), i = t == this.pattern ? 0 : t.toLowerCase() == this.folded ? -200 : null;
    return i == null ? null : (this.matched = [0, t.length], this.score = i + (e.length == this.pattern.length ? 0 : -100), this);
  }
}
const ze = /* @__PURE__ */ U.define({
  combine(n) {
    return ti(n, {
      activateOnTyping: !0,
      activateOnCompletion: () => !1,
      activateOnTypingDelay: 100,
      selectOnOpen: !0,
      override: null,
      closeOnBlur: !0,
      maxRenderedOptions: 100,
      defaultKeymap: !0,
      tooltipClass: () => "",
      optionClass: () => "",
      aboveCursor: !1,
      icons: !0,
      addToOptions: [],
      positionInfo: Vx,
      filterStrict: !1,
      compareCompletions: (e, t) => (e.sortText || e.label).localeCompare(t.sortText || t.label),
      interactionDelay: 75,
      updateSyncTime: 100
    }, {
      defaultKeymap: (e, t) => e && t,
      closeOnBlur: (e, t) => e && t,
      icons: (e, t) => e && t,
      tooltipClass: (e, t) => (i) => zc(e(i), t(i)),
      optionClass: (e, t) => (i) => zc(e(i), t(i)),
      addToOptions: (e, t) => e.concat(t),
      filterStrict: (e, t) => e || t
    });
  }
});
function zc(n, e) {
  return n ? e ? n + " " + e : n : e;
}
function Vx(n, e, t, i, r, s) {
  let o = n.textDirection == xe.RTL, l = o, a = !1, f = "top", u, g, y = e.left - r.left, b = r.right - e.right, k = i.right - i.left, C = i.bottom - i.top;
  if (l && y < Math.min(k, b) ? l = !1 : !l && b < Math.min(k, y) && (l = !0), k <= (l ? y : b))
    u = Math.max(r.top, Math.min(t.top, r.bottom - C)) - e.top, g = Math.min(400, l ? y : b);
  else {
    a = !0, g = Math.min(
      400,
      (o ? e.right : r.right - e.left) - 30
      /* Info.Margin */
    );
    let I = r.bottom - e.bottom;
    I >= C || I > e.top ? u = t.bottom - e.top : (f = "bottom", u = e.bottom - t.top);
  }
  let M = (e.bottom - e.top) / s.offsetHeight, R = (e.right - e.left) / s.offsetWidth;
  return {
    style: `${f}: ${u / M}px; max-width: ${g / R}px`,
    class: "cm-completionInfo-" + (a ? o ? "left-narrow" : "right-narrow" : l ? "left" : "right")
  };
}
function Hx(n) {
  let e = n.addToOptions.slice();
  return n.icons && e.push({
    render(t) {
      let i = document.createElement("div");
      return i.classList.add("cm-completionIcon"), t.type && i.classList.add(...t.type.split(/\s+/g).map((r) => "cm-completionIcon-" + r)), i.setAttribute("aria-hidden", "true"), i;
    },
    position: 20
  }), e.push({
    render(t, i, r, s) {
      let o = document.createElement("span");
      o.className = "cm-completionLabel";
      let l = t.displayLabel || t.label, a = 0;
      for (let f = 0; f < s.length; ) {
        let u = s[f++], g = s[f++];
        u > a && o.appendChild(document.createTextNode(l.slice(a, u)));
        let y = o.appendChild(document.createElement("span"));
        y.appendChild(document.createTextNode(l.slice(u, g))), y.className = "cm-completionMatchedText", a = g;
      }
      return a < l.length && o.appendChild(document.createTextNode(l.slice(a))), o;
    },
    position: 50
  }, {
    render(t) {
      if (!t.detail)
        return null;
      let i = document.createElement("span");
      return i.className = "cm-completionDetail", i.textContent = t.detail, i;
    },
    position: 80
  }), e.sort((t, i) => t.position - i.position).map((t) => t.render);
}
function Do(n, e, t) {
  if (n <= t)
    return { from: 0, to: n };
  if (e < 0 && (e = 0), e <= n >> 1) {
    let r = Math.floor(e / t);
    return { from: r * t, to: (r + 1) * t };
  }
  let i = Math.floor((n - e) / t);
  return { from: n - (i + 1) * t, to: n - i * t };
}
class zx {
  constructor(e, t, i) {
    this.view = e, this.stateField = t, this.applyCompletion = i, this.info = null, this.infoDestroy = null, this.placeInfoReq = {
      read: () => this.measureInfo(),
      write: (a) => this.placeInfo(a),
      key: this
    }, this.space = null, this.currentClass = "";
    let r = e.state.field(t), { options: s, selected: o } = r.open, l = e.state.facet(ze);
    this.optionContent = Hx(l), this.optionClass = l.optionClass, this.tooltipClass = l.tooltipClass, this.range = Do(s.length, o, l.maxRenderedOptions), this.dom = document.createElement("div"), this.dom.className = "cm-tooltip-autocomplete", this.updateTooltipClass(e.state), this.dom.addEventListener("mousedown", (a) => {
      let { options: f } = e.state.field(t).open;
      for (let u = a.target, g; u && u != this.dom; u = u.parentNode)
        if (u.nodeName == "LI" && (g = /-(\d+)$/.exec(u.id)) && +g[1] < f.length) {
          this.applyCompletion(e, f[+g[1]]), a.preventDefault();
          return;
        }
    }), this.dom.addEventListener("focusout", (a) => {
      let f = e.state.field(this.stateField, !1);
      f && f.tooltip && e.state.facet(ze).closeOnBlur && a.relatedTarget != e.contentDOM && e.dispatch({ effects: sr.of(null) });
    }), this.showOptions(s, r.id);
  }
  mount() {
    this.updateSel();
  }
  showOptions(e, t) {
    this.list && this.list.remove(), this.list = this.dom.appendChild(this.createListBox(e, t, this.range)), this.list.addEventListener("scroll", () => {
      this.info && this.view.requestMeasure(this.placeInfoReq);
    });
  }
  update(e) {
    var t;
    let i = e.state.field(this.stateField), r = e.startState.field(this.stateField);
    if (this.updateTooltipClass(e.state), i != r) {
      let { options: s, selected: o, disabled: l } = i.open;
      (!r.open || r.open.options != s) && (this.range = Do(s.length, o, e.state.facet(ze).maxRenderedOptions), this.showOptions(s, i.id)), this.updateSel(), l != ((t = r.open) === null || t === void 0 ? void 0 : t.disabled) && this.dom.classList.toggle("cm-tooltip-autocomplete-disabled", !!l);
    }
  }
  updateTooltipClass(e) {
    let t = this.tooltipClass(e);
    if (t != this.currentClass) {
      for (let i of this.currentClass.split(" "))
        i && this.dom.classList.remove(i);
      for (let i of t.split(" "))
        i && this.dom.classList.add(i);
      this.currentClass = t;
    }
  }
  positioned(e) {
    this.space = e, this.info && this.view.requestMeasure(this.placeInfoReq);
  }
  updateSel() {
    let e = this.view.state.field(this.stateField), t = e.open;
    (t.selected > -1 && t.selected < this.range.from || t.selected >= this.range.to) && (this.range = Do(t.options.length, t.selected, this.view.state.facet(ze).maxRenderedOptions), this.showOptions(t.options, e.id));
    let i = this.updateSelectedOption(t.selected);
    if (i) {
      this.destroyInfo();
      let { completion: r } = t.options[t.selected], { info: s } = r;
      if (!s)
        return;
      let o = typeof s == "string" ? document.createTextNode(s) : s(r);
      if (!o)
        return;
      "then" in o ? o.then((l) => {
        l && this.view.state.field(this.stateField, !1) == e && this.addInfoPane(l, r);
      }).catch((l) => ft(this.view.state, l, "completion info")) : (this.addInfoPane(o, r), i.setAttribute("aria-describedby", this.info.id));
    }
  }
  addInfoPane(e, t) {
    this.destroyInfo();
    let i = this.info = document.createElement("div");
    if (i.className = "cm-tooltip cm-completionInfo", i.id = "cm-completionInfo-" + Math.floor(Math.random() * 65535).toString(16), e.nodeType != null)
      i.appendChild(e), this.infoDestroy = null;
    else {
      let { dom: r, destroy: s } = e;
      i.appendChild(r), this.infoDestroy = s || null;
    }
    this.dom.appendChild(i), this.view.requestMeasure(this.placeInfoReq);
  }
  updateSelectedOption(e) {
    let t = null;
    for (let i = this.list.firstChild, r = this.range.from; i; i = i.nextSibling, r++)
      i.nodeName != "LI" || !i.id ? r-- : r == e ? i.hasAttribute("aria-selected") || (i.setAttribute("aria-selected", "true"), t = i) : i.hasAttribute("aria-selected") && (i.removeAttribute("aria-selected"), i.removeAttribute("aria-describedby"));
    return t && qx(this.list, t), t;
  }
  measureInfo() {
    let e = this.dom.querySelector("[aria-selected]");
    if (!e || !this.info)
      return null;
    let t = this.dom.getBoundingClientRect(), i = this.info.getBoundingClientRect(), r = e.getBoundingClientRect(), s = this.space;
    if (!s) {
      let o = this.dom.ownerDocument.documentElement;
      s = { left: 0, top: 0, right: o.clientWidth, bottom: o.clientHeight };
    }
    return r.top > Math.min(s.bottom, t.bottom) - 10 || r.bottom < Math.max(s.top, t.top) + 10 ? null : this.view.state.facet(ze).positionInfo(this.view, t, r, i, s, this.dom);
  }
  placeInfo(e) {
    this.info && (e ? (e.style && (this.info.style.cssText = e.style), this.info.className = "cm-tooltip cm-completionInfo " + (e.class || "")) : this.info.style.cssText = "top: -1e6px");
  }
  createListBox(e, t, i) {
    const r = document.createElement("ul");
    r.id = t, r.setAttribute("role", "listbox"), r.setAttribute("aria-expanded", "true"), r.setAttribute("aria-label", this.view.state.phrase("Completions")), r.addEventListener("mousedown", (o) => {
      o.target == r && o.preventDefault();
    });
    let s = null;
    for (let o = i.from; o < i.to; o++) {
      let { completion: l, match: a } = e[o], { section: f } = l;
      if (f) {
        let y = typeof f == "string" ? f : f.name;
        if (y != s && (o > i.from || i.from == 0))
          if (s = y, typeof f != "string" && f.header)
            r.appendChild(f.header(f));
          else {
            let b = r.appendChild(document.createElement("completion-section"));
            b.textContent = y;
          }
      }
      const u = r.appendChild(document.createElement("li"));
      u.id = t + "-" + o, u.setAttribute("role", "option");
      let g = this.optionClass(l);
      g && (u.className = g);
      for (let y of this.optionContent) {
        let b = y(l, this.view.state, this.view, a);
        b && u.appendChild(b);
      }
    }
    return i.from && r.classList.add("cm-completionListIncompleteTop"), i.to < e.length && r.classList.add("cm-completionListIncompleteBottom"), r;
  }
  destroyInfo() {
    this.info && (this.infoDestroy && this.infoDestroy(), this.info.remove(), this.info = null);
  }
  destroy() {
    this.destroyInfo();
  }
}
function $x(n, e) {
  return (t) => new zx(t, n, e);
}
function qx(n, e) {
  let t = n.getBoundingClientRect(), i = e.getBoundingClientRect(), r = t.height / n.offsetHeight;
  i.top < t.top ? n.scrollTop -= (t.top - i.top) / r : i.bottom > t.bottom && (n.scrollTop += (i.bottom - t.bottom) / r);
}
function $c(n) {
  return (n.boost || 0) * 100 + (n.apply ? 10 : 0) + (n.info ? 5 : 0) + (n.type ? 1 : 0);
}
function Kx(n, e) {
  let t = [], i = null, r = null, s = (u) => {
    t.push(u);
    let { section: g } = u.completion;
    if (g) {
      i || (i = []);
      let y = typeof g == "string" ? g : g.name;
      i.some((b) => b.name == y) || i.push(typeof g == "string" ? { name: y } : g);
    }
  }, o = e.facet(ze);
  for (let u of n)
    if (u.hasResult()) {
      let g = u.result.getMatch;
      if (u.result.filter === !1)
        for (let y of u.result.options)
          s(new Vc(y, u.source, g ? g(y) : [], 1e9 - t.length));
      else {
        let y = e.sliceDoc(u.from, u.to), b, k = o.filterStrict ? new Qx(y) : new Wx(y);
        for (let C of u.result.options)
          if (b = k.match(C.label)) {
            let M = C.displayLabel ? g ? g(C, b.matched) : [] : b.matched, R = b.score + (C.boost || 0);
            if (s(new Vc(C, u.source, M, R)), typeof C.section == "object" && C.section.rank === "dynamic") {
              let { name: I } = C.section;
              r || (r = /* @__PURE__ */ Object.create(null)), r[I] = Math.max(R, r[I] || -1e9);
            }
          }
      }
    }
  if (i) {
    let u = /* @__PURE__ */ Object.create(null), g = 0, y = (b, k) => (b.rank === "dynamic" && k.rank === "dynamic" ? r[k.name] - r[b.name] : 0) || (typeof b.rank == "number" ? b.rank : 1e9) - (typeof k.rank == "number" ? k.rank : 1e9) || (b.name < k.name ? -1 : 1);
    for (let b of i.sort(y))
      g -= 1e5, u[b.name] = g;
    for (let b of t) {
      let { section: k } = b.completion;
      k && (b.score += u[typeof k == "string" ? k : k.name]);
    }
  }
  let l = [], a = null, f = o.compareCompletions;
  for (let u of t.sort((g, y) => y.score - g.score || f(g.completion, y.completion))) {
    let g = u.completion;
    !a || a.label != g.label || a.detail != g.detail || a.type != null && g.type != null && a.type != g.type || a.apply != g.apply || a.boost != g.boost ? l.push(u) : $c(u.completion) > $c(a) && (l[l.length - 1] = u), a = u.completion;
  }
  return l;
}
class an {
  constructor(e, t, i, r, s, o) {
    this.options = e, this.attrs = t, this.tooltip = i, this.timestamp = r, this.selected = s, this.disabled = o;
  }
  setSelected(e, t) {
    return e == this.selected || e >= this.options.length ? this : new an(this.options, qc(t, e), this.tooltip, this.timestamp, e, this.disabled);
  }
  static build(e, t, i, r, s, o) {
    if (r && !o && e.some((f) => f.isPending))
      return r.setDisabled();
    let l = Kx(e, t);
    if (!l.length)
      return r && e.some((f) => f.isPending) ? r.setDisabled() : null;
    let a = t.facet(ze).selectOnOpen ? 0 : -1;
    if (r && r.selected != a && r.selected != -1) {
      let f = r.options[r.selected].completion;
      for (let u = 0; u < l.length; u++)
        if (l[u].completion == f) {
          a = u;
          break;
        }
    }
    return new an(l, qc(i, a), {
      pos: e.reduce((f, u) => u.hasResult() ? Math.min(f, u.from) : f, 1e8),
      create: Gx,
      above: s.aboveCursor
    }, r ? r.timestamp : Date.now(), a, !1);
  }
  map(e) {
    return new an(this.options, this.attrs, { ...this.tooltip, pos: e.mapPos(this.tooltip.pos) }, this.timestamp, this.selected, this.disabled);
  }
  setDisabled() {
    return new an(this.options, this.attrs, this.tooltip, this.timestamp, this.selected, !0);
  }
}
class Ms {
  constructor(e, t, i) {
    this.active = e, this.id = t, this.open = i;
  }
  static start() {
    return new Ms(Xx, "cm-ac-" + Math.floor(Math.random() * 2e6).toString(36), null);
  }
  update(e) {
    let { state: t } = e, i = t.facet(ze), s = (i.override || t.languageDataAt("autocomplete", $i(t)).map(Fx)).map((a) => (this.active.find((u) => u.source == a) || new Rt(
      a,
      this.active.some(
        (u) => u.state != 0
        /* State.Inactive */
      ) ? 1 : 0
      /* State.Inactive */
    )).update(e, i));
    s.length == this.active.length && s.every((a, f) => a == this.active[f]) && (s = this.active);
    let o = this.open, l = e.effects.some((a) => a.is(Oa));
    o && e.docChanged && (o = o.map(e.changes)), e.selection || s.some((a) => a.hasResult() && e.changes.touchesRange(a.from, a.to)) || !_x(s, this.active) || l ? o = an.build(s, t, this.id, o, i, l) : o && o.disabled && !s.some((a) => a.isPending) && (o = null), !o && s.every((a) => !a.isPending) && s.some((a) => a.hasResult()) && (s = s.map((a) => a.hasResult() ? new Rt(
      a.source,
      0
      /* State.Inactive */
    ) : a));
    for (let a of e.effects)
      a.is(lp) && (o = o && o.setSelected(a.value, this.id));
    return s == this.active && o == this.open ? this : new Ms(s, this.id, o);
  }
  get tooltip() {
    return this.open ? this.open.tooltip : null;
  }
  get attrs() {
    return this.open ? this.open.attrs : this.active.length ? jx : Ux;
  }
}
function _x(n, e) {
  if (n == e)
    return !0;
  for (let t = 0, i = 0; ; ) {
    for (; t < n.length && !n[t].hasResult(); )
      t++;
    for (; i < e.length && !e[i].hasResult(); )
      i++;
    let r = t == n.length, s = i == e.length;
    if (r || s)
      return r == s;
    if (n[t++].result != e[i++].result)
      return !1;
  }
}
const jx = {
  "aria-autocomplete": "list"
}, Ux = {};
function qc(n, e) {
  let t = {
    "aria-autocomplete": "list",
    "aria-haspopup": "listbox",
    "aria-controls": n
  };
  return e > -1 && (t["aria-activedescendant"] = n + "-" + e), t;
}
const Xx = [];
function op(n, e) {
  if (n.isUserEvent("input.complete")) {
    let i = n.annotation(sp);
    if (i && e.activateOnCompletion(i))
      return 12;
  }
  let t = n.isUserEvent("input.type");
  return t && e.activateOnTyping ? 5 : t ? 1 : n.isUserEvent("delete.backward") ? 2 : n.selection ? 8 : n.docChanged ? 16 : 0;
}
class Rt {
  constructor(e, t, i = !1) {
    this.source = e, this.state = t, this.explicit = i;
  }
  hasResult() {
    return !1;
  }
  get isPending() {
    return this.state == 1;
  }
  update(e, t) {
    let i = op(e, t), r = this;
    (i & 8 || i & 16 && this.touches(e)) && (r = new Rt(
      r.source,
      0
      /* State.Inactive */
    )), i & 4 && r.state == 0 && (r = new Rt(
      this.source,
      1
      /* State.Pending */
    )), r = r.updateFor(e, i);
    for (let s of e.effects)
      if (s.is(As))
        r = new Rt(r.source, 1, s.value);
      else if (s.is(sr))
        r = new Rt(
          r.source,
          0
          /* State.Inactive */
        );
      else if (s.is(Oa))
        for (let o of s.value)
          o.source == r.source && (r = o);
    return r;
  }
  updateFor(e, t) {
    return this.map(e.changes);
  }
  map(e) {
    return this;
  }
  touches(e) {
    return e.changes.touchesRange($i(e.state));
  }
}
class gn extends Rt {
  constructor(e, t, i, r, s, o) {
    super(e, 3, t), this.limit = i, this.result = r, this.from = s, this.to = o;
  }
  hasResult() {
    return !0;
  }
  updateFor(e, t) {
    var i;
    if (!(t & 3))
      return this.map(e.changes);
    let r = this.result;
    r.map && !e.changes.empty && (r = r.map(r, e.changes));
    let s = e.changes.mapPos(this.from), o = e.changes.mapPos(this.to, 1), l = $i(e.state);
    if (l > o || !r || t & 2 && ($i(e.startState) == this.from || l < this.limit))
      return new Rt(
        this.source,
        t & 4 ? 1 : 0
        /* State.Inactive */
      );
    let a = e.changes.mapPos(this.limit);
    return Yx(r.validFor, e.state, s, o) ? new gn(this.source, this.explicit, a, r, s, o) : r.update && (r = r.update(r, s, o, new np(e.state, l, !1))) ? new gn(this.source, this.explicit, a, r, r.from, (i = r.to) !== null && i !== void 0 ? i : $i(e.state)) : new Rt(this.source, 1, this.explicit);
  }
  map(e) {
    return e.empty ? this : (this.result.map ? this.result.map(this.result, e) : this.result) ? new gn(this.source, this.explicit, e.mapPos(this.limit), this.result, e.mapPos(this.from), e.mapPos(this.to, 1)) : new Rt(
      this.source,
      0
      /* State.Inactive */
    );
  }
  touches(e) {
    return e.changes.touchesRange(this.from, this.to);
  }
}
function Yx(n, e, t, i) {
  if (!n)
    return !1;
  let r = e.sliceDoc(t, i);
  return typeof n == "function" ? n(r, t, i, e) : rp(n, !0).test(r);
}
const Oa = /* @__PURE__ */ ne.define({
  map(n, e) {
    return n.map((t) => t.map(e));
  }
}), lp = /* @__PURE__ */ ne.define(), ht = /* @__PURE__ */ $e.define({
  create() {
    return Ms.start();
  },
  update(n, e) {
    return n.update(e);
  },
  provide: (n) => [
    na.from(n, (e) => e.tooltip),
    _.contentAttributes.from(n, (e) => e.attrs)
  ]
});
function Aa(n, e) {
  const t = e.completion.apply || e.completion.label;
  let i = n.state.field(ht).active.find((r) => r.source == e.source);
  return i instanceof gn ? (typeof t == "string" ? n.dispatch({
    ...Nx(n.state, t, i.from, i.to),
    annotations: sp.of(e.completion)
  }) : t(n, e.completion, i.from, i.to), !0) : !1;
}
const Gx = /* @__PURE__ */ $x(ht, Aa);
function zr(n, e = "option") {
  return (t) => {
    let i = t.state.field(ht, !1);
    if (!i || !i.open || i.open.disabled || Date.now() - i.open.timestamp < t.state.facet(ze).interactionDelay)
      return !1;
    let r = 1, s;
    e == "page" && (s = Hu(t, i.open.tooltip)) && (r = Math.max(2, Math.floor(s.dom.offsetHeight / s.dom.querySelector("li").offsetHeight) - 1));
    let { length: o } = i.open.options, l = i.open.selected > -1 ? i.open.selected + r * (n ? 1 : -1) : n ? 0 : o - 1;
    return l < 0 ? l = e == "page" ? 0 : o - 1 : l >= o && (l = e == "page" ? o - 1 : 0), t.dispatch({ effects: lp.of(l) }), !0;
  };
}
const Zx = (n) => {
  let e = n.state.field(ht, !1);
  return n.state.readOnly || !e || !e.open || e.open.selected < 0 || e.open.disabled || Date.now() - e.open.timestamp < n.state.facet(ze).interactionDelay ? !1 : Aa(n, e.open.options[e.open.selected]);
}, Bo = (n) => n.state.field(ht, !1) ? (n.dispatch({ effects: As.of(!0) }), !0) : !1, Jx = (n) => {
  let e = n.state.field(ht, !1);
  return !e || !e.active.some(
    (t) => t.state != 0
    /* State.Inactive */
  ) ? !1 : (n.dispatch({ effects: sr.of(null) }), !0);
};
class e1 {
  constructor(e, t) {
    this.active = e, this.context = t, this.time = Date.now(), this.updates = [], this.done = void 0;
  }
}
const t1 = 50, i1 = 1e3, n1 = /* @__PURE__ */ De.fromClass(class {
  constructor(n) {
    this.view = n, this.debounceUpdate = -1, this.running = [], this.debounceAccept = -1, this.pendingStart = !1, this.composing = 0;
    for (let e of n.state.field(ht).active)
      e.isPending && this.startQuery(e);
  }
  update(n) {
    let e = n.state.field(ht), t = n.state.facet(ze);
    if (!n.selectionSet && !n.docChanged && n.startState.field(ht) == e)
      return;
    let i = n.transactions.some((s) => {
      let o = op(s, t);
      return o & 8 || (s.selection || s.docChanged) && !(o & 3);
    });
    for (let s = 0; s < this.running.length; s++) {
      let o = this.running[s];
      if (i || o.context.abortOnDocChange && n.docChanged || o.updates.length + n.transactions.length > t1 && Date.now() - o.time > i1) {
        for (let l of o.context.abortListeners)
          try {
            l();
          } catch (a) {
            ft(this.view.state, a);
          }
        o.context.abortListeners = null, this.running.splice(s--, 1);
      } else
        o.updates.push(...n.transactions);
    }
    this.debounceUpdate > -1 && clearTimeout(this.debounceUpdate), n.transactions.some((s) => s.effects.some((o) => o.is(As))) && (this.pendingStart = !0);
    let r = this.pendingStart ? 50 : t.activateOnTypingDelay;
    if (this.debounceUpdate = e.active.some((s) => s.isPending && !this.running.some((o) => o.active.source == s.source)) ? setTimeout(() => this.startUpdate(), r) : -1, this.composing != 0)
      for (let s of n.transactions)
        s.isUserEvent("input.type") ? this.composing = 2 : this.composing == 2 && s.selection && (this.composing = 3);
  }
  startUpdate() {
    this.debounceUpdate = -1, this.pendingStart = !1;
    let { state: n } = this.view, e = n.field(ht);
    for (let t of e.active)
      t.isPending && !this.running.some((i) => i.active.source == t.source) && this.startQuery(t);
    this.running.length && e.open && e.open.disabled && (this.debounceAccept = setTimeout(() => this.accept(), this.view.state.facet(ze).updateSyncTime));
  }
  startQuery(n) {
    let { state: e } = this.view, t = $i(e), i = new np(e, t, n.explicit, this.view), r = new e1(n, i);
    this.running.push(r), Promise.resolve(n.source(i)).then((s) => {
      r.context.aborted || (r.done = s || null, this.scheduleAccept());
    }, (s) => {
      this.view.dispatch({ effects: sr.of(null) }), ft(this.view.state, s);
    });
  }
  scheduleAccept() {
    this.running.every((n) => n.done !== void 0) ? this.accept() : this.debounceAccept < 0 && (this.debounceAccept = setTimeout(() => this.accept(), this.view.state.facet(ze).updateSyncTime));
  }
  // For each finished query in this.running, try to create a result
  // or, if appropriate, restart the query.
  accept() {
    var n;
    this.debounceAccept > -1 && clearTimeout(this.debounceAccept), this.debounceAccept = -1;
    let e = [], t = this.view.state.facet(ze), i = this.view.state.field(ht);
    for (let r = 0; r < this.running.length; r++) {
      let s = this.running[r];
      if (s.done === void 0)
        continue;
      if (this.running.splice(r--, 1), s.done) {
        let l = $i(s.updates.length ? s.updates[0].startState : this.view.state), a = Math.min(l, s.done.from + (s.active.explicit ? 0 : 1)), f = new gn(s.active.source, s.active.explicit, a, s.done, s.done.from, (n = s.done.to) !== null && n !== void 0 ? n : l);
        for (let u of s.updates)
          f = f.update(u, t);
        if (f.hasResult()) {
          e.push(f);
          continue;
        }
      }
      let o = i.active.find((l) => l.source == s.active.source);
      if (o && o.isPending)
        if (s.done == null) {
          let l = new Rt(
            s.active.source,
            0
            /* State.Inactive */
          );
          for (let a of s.updates)
            l = l.update(a, t);
          l.isPending || e.push(l);
        } else
          this.startQuery(o);
    }
    (e.length || i.open && i.open.disabled) && this.view.dispatch({ effects: Oa.of(e) });
  }
}, {
  eventHandlers: {
    blur(n) {
      let e = this.view.state.field(ht, !1);
      if (e && e.tooltip && this.view.state.facet(ze).closeOnBlur) {
        let t = e.open && Hu(this.view, e.open.tooltip);
        (!t || !t.dom.contains(n.relatedTarget)) && setTimeout(() => this.view.dispatch({ effects: sr.of(null) }), 10);
      }
    },
    compositionstart() {
      this.composing = 1;
    },
    compositionend() {
      this.composing == 3 && setTimeout(() => this.view.dispatch({ effects: As.of(!1) }), 20), this.composing = 0;
    }
  }
}), r1 = typeof navigator == "object" && /* @__PURE__ */ /Win/.test(navigator.platform), s1 = /* @__PURE__ */ Ti.highest(/* @__PURE__ */ _.domEventHandlers({
  keydown(n, e) {
    let t = e.state.field(ht, !1);
    if (!t || !t.open || t.open.disabled || t.open.selected < 0 || n.key.length > 1 || n.ctrlKey && !(r1 && n.altKey) || n.metaKey)
      return !1;
    let i = t.open.options[t.open.selected], r = t.active.find((o) => o.source == i.source), s = i.completion.commitCharacters || r.result.commitCharacters;
    return s && s.indexOf(n.key) > -1 && Aa(e, i), !1;
  }
})), o1 = /* @__PURE__ */ _.baseTheme({
  ".cm-tooltip.cm-tooltip-autocomplete": {
    "& > ul": {
      fontFamily: "monospace",
      whiteSpace: "nowrap",
      overflow: "hidden auto",
      maxWidth_fallback: "700px",
      maxWidth: "min(700px, 95vw)",
      minWidth: "250px",
      maxHeight: "10em",
      height: "100%",
      listStyle: "none",
      margin: 0,
      padding: 0,
      "& > li, & > completion-section": {
        padding: "1px 3px",
        lineHeight: 1.2
      },
      "& > li": {
        overflowX: "hidden",
        textOverflow: "ellipsis",
        cursor: "pointer"
      },
      "& > completion-section": {
        display: "list-item",
        borderBottom: "1px solid silver",
        paddingLeft: "0.5em",
        opacity: 0.7
      }
    }
  },
  "&light .cm-tooltip-autocomplete ul li[aria-selected]": {
    background: "#17c",
    color: "white"
  },
  "&light .cm-tooltip-autocomplete-disabled ul li[aria-selected]": {
    background: "#777"
  },
  "&dark .cm-tooltip-autocomplete ul li[aria-selected]": {
    background: "#347",
    color: "white"
  },
  "&dark .cm-tooltip-autocomplete-disabled ul li[aria-selected]": {
    background: "#444"
  },
  ".cm-completionListIncompleteTop:before, .cm-completionListIncompleteBottom:after": {
    content: '"···"',
    opacity: 0.5,
    display: "block",
    textAlign: "center"
  },
  ".cm-tooltip.cm-completionInfo": {
    position: "absolute",
    padding: "3px 9px",
    width: "max-content",
    maxWidth: "400px",
    boxSizing: "border-box",
    whiteSpace: "pre-line"
  },
  ".cm-completionInfo.cm-completionInfo-left": { right: "100%" },
  ".cm-completionInfo.cm-completionInfo-right": { left: "100%" },
  ".cm-completionInfo.cm-completionInfo-left-narrow": { right: "30px" },
  ".cm-completionInfo.cm-completionInfo-right-narrow": { left: "30px" },
  "&light .cm-snippetField": { backgroundColor: "#00000022" },
  "&dark .cm-snippetField": { backgroundColor: "#ffffff22" },
  ".cm-snippetFieldPosition": {
    verticalAlign: "text-top",
    width: 0,
    height: "1.15em",
    display: "inline-block",
    margin: "0 -0.7px -.7em",
    borderLeft: "1.4px dotted #888"
  },
  ".cm-completionMatchedText": {
    textDecoration: "underline"
  },
  ".cm-completionDetail": {
    marginLeft: "0.5em",
    fontStyle: "italic"
  },
  ".cm-completionIcon": {
    fontSize: "90%",
    width: ".8em",
    display: "inline-block",
    textAlign: "center",
    paddingRight: ".6em",
    opacity: "0.6",
    boxSizing: "content-box"
  },
  ".cm-completionIcon-function, .cm-completionIcon-method": {
    "&:after": { content: "'ƒ'" }
  },
  ".cm-completionIcon-class": {
    "&:after": { content: "'○'" }
  },
  ".cm-completionIcon-interface": {
    "&:after": { content: "'◌'" }
  },
  ".cm-completionIcon-variable": {
    "&:after": { content: "'𝑥'" }
  },
  ".cm-completionIcon-constant": {
    "&:after": { content: "'𝐶'" }
  },
  ".cm-completionIcon-type": {
    "&:after": { content: "'𝑡'" }
  },
  ".cm-completionIcon-enum": {
    "&:after": { content: "'∪'" }
  },
  ".cm-completionIcon-property": {
    "&:after": { content: "'□'" }
  },
  ".cm-completionIcon-keyword": {
    "&:after": { content: "'🔑︎'" }
    // Disable emoji rendering
  },
  ".cm-completionIcon-namespace": {
    "&:after": { content: "'▢'" }
  },
  ".cm-completionIcon-text": {
    "&:after": { content: "'abc'", fontSize: "50%", verticalAlign: "middle" }
  }
}), or = {
  brackets: ["(", "[", "{", "'", '"'],
  before: ")]}:;>",
  stringPrefixes: []
}, zi = /* @__PURE__ */ ne.define({
  map(n, e) {
    let t = e.mapPos(n, -1, Xe.TrackAfter);
    return t ?? void 0;
  }
}), Ma = /* @__PURE__ */ new class extends ki {
}();
Ma.startSide = 1;
Ma.endSide = -1;
const ap = /* @__PURE__ */ $e.define({
  create() {
    return ce.empty;
  },
  update(n, e) {
    if (n = n.map(e.changes), e.selection) {
      let t = e.state.doc.lineAt(e.selection.main.head);
      n = n.update({ filter: (i) => i >= t.from && i <= t.to });
    }
    for (let t of e.effects)
      t.is(zi) && (n = n.update({ add: [Ma.range(t.value, t.value + 1)] }));
    return n;
  }
});
function l1() {
  return [h1, ap];
}
const Eo = "()[]{}<>«»»«［］｛｝";
function hp(n) {
  for (let e = 0; e < Eo.length; e += 2)
    if (Eo.charCodeAt(e) == n)
      return Eo.charAt(e + 1);
  return Ql(n < 128 ? n : n + 1);
}
function cp(n, e) {
  return n.languageDataAt("closeBrackets", e)[0] || or;
}
const a1 = typeof navigator == "object" && /* @__PURE__ */ /Android\b/.test(navigator.userAgent), h1 = /* @__PURE__ */ _.inputHandler.of((n, e, t, i) => {
  if ((a1 ? n.composing : n.compositionStarted) || n.state.readOnly)
    return !1;
  let r = n.state.selection.main;
  if (i.length > 2 || i.length == 2 && Yt(at(i, 0)) == 1 || e != r.from || t != r.to)
    return !1;
  let s = u1(n.state, i);
  return s ? (n.dispatch(s), !0) : !1;
}), c1 = ({ state: n, dispatch: e }) => {
  if (n.readOnly)
    return !1;
  let i = cp(n, n.selection.main.head).brackets || or.brackets, r = null, s = n.changeByRange((o) => {
    if (o.empty) {
      let l = d1(n.doc, o.head);
      for (let a of i)
        if (a == l && Us(n.doc, o.head) == hp(at(a, 0)))
          return {
            changes: { from: o.head - a.length, to: o.head + a.length },
            range: E.cursor(o.head - a.length)
          };
    }
    return { range: r = o };
  });
  return r || e(n.update(s, { scrollIntoView: !0, userEvent: "delete.backward" })), !r;
}, f1 = [
  { key: "Backspace", run: c1 }
];
function u1(n, e) {
  let t = cp(n, n.selection.main.head), i = t.brackets || or.brackets;
  for (let r of i) {
    let s = hp(at(r, 0));
    if (e == r)
      return s == r ? m1(n, r, i.indexOf(r + r + r) > -1, t) : p1(n, r, s, t.before || or.before);
    if (e == s && fp(n, n.selection.main.from))
      return g1(n, r, s);
  }
  return null;
}
function fp(n, e) {
  let t = !1;
  return n.field(ap).between(0, n.doc.length, (i) => {
    i == e && (t = !0);
  }), t;
}
function Us(n, e) {
  let t = n.sliceString(e, e + 2);
  return t.slice(0, Yt(at(t, 0)));
}
function d1(n, e) {
  let t = n.sliceString(e - 2, e);
  return Yt(at(t, 0)) == t.length ? t : t.slice(1);
}
function p1(n, e, t, i) {
  let r = null, s = n.changeByRange((o) => {
    if (!o.empty)
      return {
        changes: [{ insert: e, from: o.from }, { insert: t, from: o.to }],
        effects: zi.of(o.to + e.length),
        range: E.range(o.anchor + e.length, o.head + e.length)
      };
    let l = Us(n.doc, o.head);
    return !l || /\s/.test(l) || i.indexOf(l) > -1 ? {
      changes: { insert: e + t, from: o.head },
      effects: zi.of(o.head + e.length),
      range: E.cursor(o.head + e.length)
    } : { range: r = o };
  });
  return r ? null : n.update(s, {
    scrollIntoView: !0,
    userEvent: "input.type"
  });
}
function g1(n, e, t) {
  let i = null, r = n.changeByRange((s) => s.empty && Us(n.doc, s.head) == t ? {
    changes: { from: s.head, to: s.head + t.length, insert: t },
    range: E.cursor(s.head + t.length)
  } : i = { range: s });
  return i ? null : n.update(r, {
    scrollIntoView: !0,
    userEvent: "input.type"
  });
}
function m1(n, e, t, i) {
  let r = i.stringPrefixes || or.stringPrefixes, s = null, o = n.changeByRange((l) => {
    if (!l.empty)
      return {
        changes: [{ insert: e, from: l.from }, { insert: e, from: l.to }],
        effects: zi.of(l.to + e.length),
        range: E.range(l.anchor + e.length, l.head + e.length)
      };
    let a = l.head, f = Us(n.doc, a), u;
    if (f == e) {
      if (Kc(n, a))
        return {
          changes: { insert: e + e, from: a },
          effects: zi.of(a + e.length),
          range: E.cursor(a + e.length)
        };
      if (fp(n, a)) {
        let y = t && n.sliceDoc(a, a + e.length * 3) == e + e + e ? e + e + e : e;
        return {
          changes: { from: a, to: a + y.length, insert: y },
          range: E.cursor(a + y.length)
        };
      }
    } else {
      if (t && n.sliceDoc(a - 2 * e.length, a) == e + e && (u = _c(n, a - 2 * e.length, r)) > -1 && Kc(n, u))
        return {
          changes: { insert: e + e + e + e, from: a },
          effects: zi.of(a + e.length),
          range: E.cursor(a + e.length)
        };
      if (n.charCategorizer(a)(f) != Me.Word && _c(n, a, r) > -1 && !v1(n, a, e, r))
        return {
          changes: { insert: e + e, from: a },
          effects: zi.of(a + e.length),
          range: E.cursor(a + e.length)
        };
    }
    return { range: s = l };
  });
  return s ? null : n.update(o, {
    scrollIntoView: !0,
    userEvent: "input.type"
  });
}
function Kc(n, e) {
  let t = Ve(n).resolveInner(e + 1);
  return t.parent && t.from == e;
}
function v1(n, e, t, i) {
  let r = Ve(n).resolveInner(e, -1), s = i.reduce((o, l) => Math.max(o, l.length), 0);
  for (let o = 0; o < 5; o++) {
    let l = n.sliceDoc(r.from, Math.min(r.to, r.from + t.length + s)), a = l.indexOf(t);
    if (!a || a > -1 && i.indexOf(l.slice(0, a)) > -1) {
      let u = r.firstChild;
      for (; u && u.from == r.from && u.to - u.from > t.length + a; ) {
        if (n.sliceDoc(u.to - t.length, u.to) == t)
          return !1;
        u = u.firstChild;
      }
      return !0;
    }
    let f = r.to == e && r.parent;
    if (!f)
      break;
    r = f;
  }
  return !1;
}
function _c(n, e, t) {
  let i = n.charCategorizer(e);
  if (i(n.sliceDoc(e - 1, e)) != Me.Word)
    return e;
  for (let r of t) {
    let s = e - r.length;
    if (n.sliceDoc(s, e) == r && i(n.sliceDoc(s - 1, s)) != Me.Word)
      return s;
  }
  return -1;
}
function y1(n = {}) {
  return [
    s1,
    ht,
    ze.of(n),
    n1,
    b1,
    o1
  ];
}
const up = [
  { key: "Ctrl-Space", run: Bo },
  { mac: "Alt-`", run: Bo },
  { mac: "Alt-i", run: Bo },
  { key: "Escape", run: Jx },
  { key: "ArrowDown", run: /* @__PURE__ */ zr(!0) },
  { key: "ArrowUp", run: /* @__PURE__ */ zr(!1) },
  { key: "PageDown", run: /* @__PURE__ */ zr(!0, "page") },
  { key: "PageUp", run: /* @__PURE__ */ zr(!1, "page") },
  { key: "Enter", run: Zx }
], b1 = /* @__PURE__ */ Ti.highest(/* @__PURE__ */ ta.computeN([ze], (n) => n.facet(ze).defaultKeymap ? [up] : []));
class jc {
  constructor(e, t, i) {
    this.from = e, this.to = t, this.diagnostic = i;
  }
}
class Qi {
  constructor(e, t, i) {
    this.diagnostics = e, this.panel = t, this.selected = i;
  }
  static init(e, t, i) {
    let r = i.facet(lr).markerFilter;
    r && (e = r(e, i));
    let s = e.slice().sort((b, k) => b.from - k.from || b.to - k.to), o = new ei(), l = [], a = 0, f = i.doc.iter(), u = 0, g = i.doc.length;
    for (let b = 0; ; ) {
      let k = b == s.length ? null : s[b];
      if (!k && !l.length)
        break;
      let C, M;
      if (l.length)
        C = a, M = l.reduce((N, z) => Math.min(N, z.to), k && k.from > C ? k.from : 1e8);
      else {
        if (C = k.from, C > g)
          break;
        M = k.to, l.push(k), b++;
      }
      for (; b < s.length; ) {
        let N = s[b];
        if (N.from == C && (N.to > N.from || N.to == C))
          l.push(N), b++, M = Math.min(N.to, M);
        else {
          M = Math.min(N.from, M);
          break;
        }
      }
      M = Math.min(M, g);
      let R = !1;
      if (l.some((N) => N.from == C && (N.to == M || M == g)) && (R = C == M, !R && M - C < 10)) {
        let N = C - (u + f.value.length);
        N > 0 && (f.next(N), u = C);
        for (let z = C; ; ) {
          if (z >= M) {
            R = !0;
            break;
          }
          if (!f.lineBreak && u + f.value.length > z)
            break;
          z = u + f.value.length, u += f.value.length, f.next();
        }
      }
      let I = D1(l);
      if (R)
        o.add(C, C, G.widget({
          widget: new T1(I),
          diagnostics: l.slice()
        }));
      else {
        let N = l.reduce((z, F) => F.markClass ? z + " " + F.markClass : z, "");
        o.add(C, M, G.mark({
          class: "cm-lintRange cm-lintRange-" + I + N,
          diagnostics: l.slice(),
          inclusiveEnd: l.some((z) => z.to > M)
        }));
      }
      if (a = M, a == g)
        break;
      for (let N = 0; N < l.length; N++)
        l[N].to <= a && l.splice(N--, 1);
    }
    let y = o.finish();
    return new Qi(y, t, On(y));
  }
}
function On(n, e = null, t = 0) {
  let i = null;
  return n.between(t, 1e9, (r, s, { spec: o }) => {
    if (!(e && o.diagnostics.indexOf(e) < 0))
      if (!i)
        i = new jc(r, s, e || o.diagnostics[0]);
      else {
        if (o.diagnostics.indexOf(i.diagnostic) < 0)
          return !1;
        i = new jc(i.from, s, i.diagnostic);
      }
  }), i;
}
function x1(n, e) {
  let t = e.pos, i = e.end || t, r = n.state.facet(lr).hideOn(n, t, i);
  if (r != null)
    return r;
  let s = n.startState.doc.lineAt(e.pos);
  return !!(n.effects.some((o) => o.is(dp)) || n.changes.touchesRange(s.from, Math.max(s.to, i)));
}
function k1(n, e) {
  return n.field(yt, !1) ? e : e.concat(ne.appendConfig.of(B1));
}
const dp = /* @__PURE__ */ ne.define(), Ta = /* @__PURE__ */ ne.define(), pp = /* @__PURE__ */ ne.define(), yt = /* @__PURE__ */ $e.define({
  create() {
    return new Qi(G.none, null, null);
  },
  update(n, e) {
    if (e.docChanged && n.diagnostics.size) {
      let t = n.diagnostics.map(e.changes), i = null, r = n.panel;
      if (n.selected) {
        let s = e.changes.mapPos(n.selected.from, 1);
        i = On(t, n.selected.diagnostic, s) || On(t, null, s);
      }
      !t.size && r && e.state.facet(lr).autoPanel && (r = null), n = new Qi(t, r, i);
    }
    for (let t of e.effects)
      if (t.is(dp)) {
        let i = e.state.facet(lr).autoPanel ? t.value.length ? ar.open : null : n.panel;
        n = Qi.init(t.value, i, e.state);
      } else t.is(Ta) ? n = new Qi(n.diagnostics, t.value ? ar.open : null, n.selected) : t.is(pp) && (n = new Qi(n.diagnostics, n.panel, t.value));
    return n;
  },
  provide: (n) => [
    ji.from(n, (e) => e.panel),
    _.decorations.from(n, (e) => e.diagnostics)
  ]
}), w1 = /* @__PURE__ */ G.mark({ class: "cm-lintRange cm-lintRange-active" });
function S1(n, e, t) {
  let { diagnostics: i } = n.state.field(yt), r, s = -1, o = -1;
  i.between(e - (t < 0 ? 1 : 0), e + (t > 0 ? 1 : 0), (a, f, { spec: u }) => {
    if (e >= a && e <= f && (a == f || (e > a || t > 0) && (e < f || t < 0)))
      return r = u.diagnostics, s = a, o = f, !1;
  });
  let l = n.state.facet(lr).tooltipFilter;
  return r && l && (r = l(r, n.state)), r ? {
    pos: s,
    end: o,
    above: n.state.doc.lineAt(s).to < o,
    create() {
      return { dom: C1(n, r) };
    }
  } : null;
}
function C1(n, e) {
  return ye("ul", { class: "cm-tooltip-lint" }, e.map((t) => mp(n, t, !1)));
}
const O1 = (n) => {
  let e = n.state.field(yt, !1);
  (!e || !e.panel) && n.dispatch({ effects: k1(n.state, [Ta.of(!0)]) });
  let t = ra(n, ar.open);
  return t && t.dom.querySelector(".cm-panel-lint ul").focus(), !0;
}, Uc = (n) => {
  let e = n.state.field(yt, !1);
  return !e || !e.panel ? !1 : (n.dispatch({ effects: Ta.of(!1) }), !0);
}, A1 = (n) => {
  let e = n.state.field(yt, !1);
  if (!e)
    return !1;
  let t = n.state.selection.main, i = e.diagnostics.iter(t.to + 1);
  return !i.value && (i = e.diagnostics.iter(0), !i.value || i.from == t.from && i.to == t.to) ? !1 : (n.dispatch({ selection: { anchor: i.from, head: i.to }, scrollIntoView: !0 }), !0);
}, M1 = [
  { key: "Mod-Shift-m", run: O1, preventDefault: !0 },
  { key: "F8", run: A1 }
], lr = /* @__PURE__ */ U.define({
  combine(n) {
    return {
      sources: n.map((e) => e.source).filter((e) => e != null),
      ...ti(n.map((e) => e.config), {
        delay: 750,
        markerFilter: null,
        tooltipFilter: null,
        needsRefresh: null,
        hideOn: () => null
      }, {
        delay: Math.max,
        markerFilter: Xc,
        tooltipFilter: Xc,
        needsRefresh: (e, t) => e ? t ? (i) => e(i) || t(i) : e : t,
        hideOn: (e, t) => e ? t ? (i, r, s) => e(i, r, s) || t(i, r, s) : e : t,
        autoPanel: (e, t) => e || t
      })
    };
  }
});
function Xc(n, e) {
  return n ? e ? (t, i) => e(n(t, i), i) : n : e;
}
function gp(n) {
  let e = [];
  if (n)
    e: for (let { name: t } of n) {
      for (let i = 0; i < t.length; i++) {
        let r = t[i];
        if (/[a-zA-Z]/.test(r) && !e.some((s) => s.toLowerCase() == r.toLowerCase())) {
          e.push(r);
          continue e;
        }
      }
      e.push("");
    }
  return e;
}
function mp(n, e, t) {
  var i;
  let r = t ? gp(e.actions) : [];
  return ye("li", { class: "cm-diagnostic cm-diagnostic-" + e.severity }, ye("span", { class: "cm-diagnosticText" }, e.renderMessage ? e.renderMessage(n) : e.message), (i = e.actions) === null || i === void 0 ? void 0 : i.map((s, o) => {
    let l = !1, a = (b) => {
      if (b.preventDefault(), l)
        return;
      l = !0;
      let k = On(n.state.field(yt).diagnostics, e);
      k && s.apply(n, k.from, k.to);
    }, { name: f } = s, u = r[o] ? f.indexOf(r[o]) : -1, g = u < 0 ? f : [
      f.slice(0, u),
      ye("u", f.slice(u, u + 1)),
      f.slice(u + 1)
    ], y = s.markClass ? " " + s.markClass : "";
    return ye("button", {
      type: "button",
      class: "cm-diagnosticAction" + y,
      onclick: a,
      onmousedown: a,
      "aria-label": ` Action: ${f}${u < 0 ? "" : ` (access key "${r[o]})"`}.`
    }, g);
  }), e.source && ye("div", { class: "cm-diagnosticSource" }, e.source));
}
class T1 extends ci {
  constructor(e) {
    super(), this.sev = e;
  }
  eq(e) {
    return e.sev == this.sev;
  }
  toDOM() {
    return ye("span", { class: "cm-lintPoint cm-lintPoint-" + this.sev });
  }
}
class Yc {
  constructor(e, t) {
    this.diagnostic = t, this.id = "item_" + Math.floor(Math.random() * 4294967295).toString(16), this.dom = mp(e, t, !0), this.dom.id = this.id, this.dom.setAttribute("role", "option");
  }
}
class ar {
  constructor(e) {
    this.view = e, this.items = [];
    let t = (r) => {
      if (r.keyCode == 27)
        Uc(this.view), this.view.focus();
      else if (r.keyCode == 38 || r.keyCode == 33)
        this.moveSelection((this.selectedIndex - 1 + this.items.length) % this.items.length);
      else if (r.keyCode == 40 || r.keyCode == 34)
        this.moveSelection((this.selectedIndex + 1) % this.items.length);
      else if (r.keyCode == 36)
        this.moveSelection(0);
      else if (r.keyCode == 35)
        this.moveSelection(this.items.length - 1);
      else if (r.keyCode == 13)
        this.view.focus();
      else if (r.keyCode >= 65 && r.keyCode <= 90 && this.selectedIndex >= 0) {
        let { diagnostic: s } = this.items[this.selectedIndex], o = gp(s.actions);
        for (let l = 0; l < o.length; l++)
          if (o[l].toUpperCase().charCodeAt(0) == r.keyCode) {
            let a = On(this.view.state.field(yt).diagnostics, s);
            a && s.actions[l].apply(e, a.from, a.to);
          }
      } else
        return;
      r.preventDefault();
    }, i = (r) => {
      for (let s = 0; s < this.items.length; s++)
        this.items[s].dom.contains(r.target) && this.moveSelection(s);
    };
    this.list = ye("ul", {
      tabIndex: 0,
      role: "listbox",
      "aria-label": this.view.state.phrase("Diagnostics"),
      onkeydown: t,
      onclick: i
    }), this.dom = ye("div", { class: "cm-panel-lint" }, this.list, ye("button", {
      type: "button",
      name: "close",
      "aria-label": this.view.state.phrase("close"),
      onclick: () => Uc(this.view)
    }, "×")), this.update();
  }
  get selectedIndex() {
    let e = this.view.state.field(yt).selected;
    if (!e)
      return -1;
    for (let t = 0; t < this.items.length; t++)
      if (this.items[t].diagnostic == e.diagnostic)
        return t;
    return -1;
  }
  update() {
    let { diagnostics: e, selected: t } = this.view.state.field(yt), i = 0, r = !1, s = null, o = /* @__PURE__ */ new Set();
    for (e.between(0, this.view.state.doc.length, (l, a, { spec: f }) => {
      for (let u of f.diagnostics) {
        if (o.has(u))
          continue;
        o.add(u);
        let g = -1, y;
        for (let b = i; b < this.items.length; b++)
          if (this.items[b].diagnostic == u) {
            g = b;
            break;
          }
        g < 0 ? (y = new Yc(this.view, u), this.items.splice(i, 0, y), r = !0) : (y = this.items[g], g > i && (this.items.splice(i, g - i), r = !0)), t && y.diagnostic == t.diagnostic ? y.dom.hasAttribute("aria-selected") || (y.dom.setAttribute("aria-selected", "true"), s = y) : y.dom.hasAttribute("aria-selected") && y.dom.removeAttribute("aria-selected"), i++;
      }
    }); i < this.items.length && !(this.items.length == 1 && this.items[0].diagnostic.from < 0); )
      r = !0, this.items.pop();
    this.items.length == 0 && (this.items.push(new Yc(this.view, {
      from: -1,
      to: -1,
      severity: "info",
      message: this.view.state.phrase("No diagnostics")
    })), r = !0), s ? (this.list.setAttribute("aria-activedescendant", s.id), this.view.requestMeasure({
      key: this,
      read: () => ({ sel: s.dom.getBoundingClientRect(), panel: this.list.getBoundingClientRect() }),
      write: ({ sel: l, panel: a }) => {
        let f = a.height / this.list.offsetHeight;
        l.top < a.top ? this.list.scrollTop -= (a.top - l.top) / f : l.bottom > a.bottom && (this.list.scrollTop += (l.bottom - a.bottom) / f);
      }
    })) : this.selectedIndex < 0 && this.list.removeAttribute("aria-activedescendant"), r && this.sync();
  }
  sync() {
    let e = this.list.firstChild;
    function t() {
      let i = e;
      e = i.nextSibling, i.remove();
    }
    for (let i of this.items)
      if (i.dom.parentNode == this.list) {
        for (; e != i.dom; )
          t();
        e = i.dom.nextSibling;
      } else
        this.list.insertBefore(i.dom, e);
    for (; e; )
      t();
  }
  moveSelection(e) {
    if (this.selectedIndex < 0)
      return;
    let t = this.view.state.field(yt), i = On(t.diagnostics, this.items[e].diagnostic);
    i && this.view.dispatch({
      selection: { anchor: i.from, head: i.to },
      scrollIntoView: !0,
      effects: pp.of(i)
    });
  }
  static open(e) {
    return new ar(e);
  }
}
function P1(n, e = 'viewBox="0 0 40 40"') {
  return `url('data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" ${e}>${encodeURIComponent(n)}</svg>')`;
}
function $r(n) {
  return P1(`<path d="m0 2.5 l2 -1.5 l1 0 l2 1.5 l1 0" stroke="${n}" fill="none" stroke-width=".7"/>`, 'width="6" height="3"');
}
const R1 = /* @__PURE__ */ _.baseTheme({
  ".cm-diagnostic": {
    padding: "3px 6px 3px 8px",
    marginLeft: "-1px",
    display: "block",
    whiteSpace: "pre-wrap"
  },
  ".cm-diagnostic-error": { borderLeft: "5px solid #d11" },
  ".cm-diagnostic-warning": { borderLeft: "5px solid orange" },
  ".cm-diagnostic-info": { borderLeft: "5px solid #999" },
  ".cm-diagnostic-hint": { borderLeft: "5px solid #66d" },
  ".cm-diagnosticAction": {
    font: "inherit",
    border: "none",
    padding: "2px 4px",
    backgroundColor: "#444",
    color: "white",
    borderRadius: "3px",
    marginLeft: "8px",
    cursor: "pointer"
  },
  ".cm-diagnosticSource": {
    fontSize: "70%",
    opacity: 0.7
  },
  ".cm-lintRange": {
    backgroundPosition: "left bottom",
    backgroundRepeat: "repeat-x",
    paddingBottom: "0.7px"
  },
  ".cm-lintRange-error": { backgroundImage: /* @__PURE__ */ $r("#d11") },
  ".cm-lintRange-warning": { backgroundImage: /* @__PURE__ */ $r("orange") },
  ".cm-lintRange-info": { backgroundImage: /* @__PURE__ */ $r("#999") },
  ".cm-lintRange-hint": { backgroundImage: /* @__PURE__ */ $r("#66d") },
  ".cm-lintRange-active": { backgroundColor: "#ffdd9980" },
  ".cm-tooltip-lint": {
    padding: 0,
    margin: 0
  },
  ".cm-lintPoint": {
    position: "relative",
    "&:after": {
      content: '""',
      position: "absolute",
      bottom: 0,
      left: "-2px",
      borderLeft: "3px solid transparent",
      borderRight: "3px solid transparent",
      borderBottom: "4px solid #d11"
    }
  },
  ".cm-lintPoint-warning": {
    "&:after": { borderBottomColor: "orange" }
  },
  ".cm-lintPoint-info": {
    "&:after": { borderBottomColor: "#999" }
  },
  ".cm-lintPoint-hint": {
    "&:after": { borderBottomColor: "#66d" }
  },
  ".cm-panel.cm-panel-lint": {
    position: "relative",
    "& ul": {
      maxHeight: "100px",
      overflowY: "auto",
      "& [aria-selected]": {
        backgroundColor: "#ddd",
        "& u": { textDecoration: "underline" }
      },
      "&:focus [aria-selected]": {
        background_fallback: "#bdf",
        backgroundColor: "Highlight",
        color_fallback: "white",
        color: "HighlightText"
      },
      "& u": { textDecoration: "none" },
      padding: 0,
      margin: 0
    },
    "& [name=close]": {
      position: "absolute",
      top: "0",
      right: "2px",
      background: "inherit",
      border: "none",
      font: "inherit",
      padding: 0,
      margin: 0
    }
  }
});
function L1(n) {
  return n == "error" ? 4 : n == "warning" ? 3 : n == "info" ? 2 : 1;
}
function D1(n) {
  let e = "hint", t = 1;
  for (let i of n) {
    let r = L1(i.severity);
    r > t && (t = r, e = i.severity);
  }
  return e;
}
const B1 = [
  yt,
  /* @__PURE__ */ _.decorations.compute([yt], (n) => {
    let { selected: e, panel: t } = n.field(yt);
    return !e || !t || e.from == e.to ? G.none : G.set([
      w1.range(e.from, e.to)
    ]);
  }),
  /* @__PURE__ */ kv(S1, { hideOn: x1 }),
  R1
], Lw = [
  Ev(),
  Fv(),
  Z0(),
  rb(),
  Ry(),
  V0(),
  _0(),
  pe.allowMultipleSelections.of(!0),
  by(),
  dd(Ey, { fallback: !0 }),
  Hy(),
  l1(),
  y1(),
  cv(),
  dv(),
  rv(),
  hx(),
  ta.of([
    ...f1,
    ...rx,
    ...Rx,
    ...db,
    ...Ay,
    ...up,
    ...M1
  ])
], E1 = "#e5c07b", Gc = "#e06c75", I1 = "#56b6c2", N1 = "#ffffff", Jr = "#abb2bf", Dl = "#7d8799", F1 = "#61afef", W1 = "#98c379", Zc = "#d19a66", Q1 = "#c678dd", V1 = "#21252b", Jc = "#2c313a", ef = "#282c34", Io = "#353a42", H1 = "#3E4451", tf = "#528bff", z1 = /* @__PURE__ */ _.theme({
  "&": {
    color: Jr,
    backgroundColor: ef
  },
  ".cm-content": {
    caretColor: tf
  },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: tf },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": { backgroundColor: H1 },
  ".cm-panels": { backgroundColor: V1, color: Jr },
  ".cm-panels.cm-panels-top": { borderBottom: "2px solid black" },
  ".cm-panels.cm-panels-bottom": { borderTop: "2px solid black" },
  ".cm-searchMatch": {
    backgroundColor: "#72a1ff59",
    outline: "1px solid #457dff"
  },
  ".cm-searchMatch.cm-searchMatch-selected": {
    backgroundColor: "#6199ff2f"
  },
  ".cm-activeLine": { backgroundColor: "#6699ff0b" },
  ".cm-selectionMatch": { backgroundColor: "#aafe661a" },
  "&.cm-focused .cm-matchingBracket, &.cm-focused .cm-nonmatchingBracket": {
    backgroundColor: "#bad0f847"
  },
  ".cm-gutters": {
    backgroundColor: ef,
    color: Dl,
    border: "none"
  },
  ".cm-activeLineGutter": {
    backgroundColor: Jc
  },
  ".cm-foldPlaceholder": {
    backgroundColor: "transparent",
    border: "none",
    color: "#ddd"
  },
  ".cm-tooltip": {
    border: "none",
    backgroundColor: Io
  },
  ".cm-tooltip .cm-tooltip-arrow:before": {
    borderTopColor: "transparent",
    borderBottomColor: "transparent"
  },
  ".cm-tooltip .cm-tooltip-arrow:after": {
    borderTopColor: Io,
    borderBottomColor: Io
  },
  ".cm-tooltip-autocomplete": {
    "& > ul > li[aria-selected]": {
      backgroundColor: Jc,
      color: Jr
    }
  }
}, { dark: !0 }), $1 = /* @__PURE__ */ gr.define([
  {
    tag: P.keyword,
    color: Q1
  },
  {
    tag: [P.name, P.deleted, P.character, P.propertyName, P.macroName],
    color: Gc
  },
  {
    tag: [/* @__PURE__ */ P.function(P.variableName), P.labelName],
    color: F1
  },
  {
    tag: [P.color, /* @__PURE__ */ P.constant(P.name), /* @__PURE__ */ P.standard(P.name)],
    color: Zc
  },
  {
    tag: [/* @__PURE__ */ P.definition(P.name), P.separator],
    color: Jr
  },
  {
    tag: [P.typeName, P.className, P.number, P.changed, P.annotation, P.modifier, P.self, P.namespace],
    color: E1
  },
  {
    tag: [P.operator, P.operatorKeyword, P.url, P.escape, P.regexp, P.link, /* @__PURE__ */ P.special(P.string)],
    color: I1
  },
  {
    tag: [P.meta, P.comment],
    color: Dl
  },
  {
    tag: P.strong,
    fontWeight: "bold"
  },
  {
    tag: P.emphasis,
    fontStyle: "italic"
  },
  {
    tag: P.strikethrough,
    textDecoration: "line-through"
  },
  {
    tag: P.link,
    color: Dl,
    textDecoration: "underline"
  },
  {
    tag: P.heading,
    fontWeight: "bold",
    color: Gc
  },
  {
    tag: [P.atom, P.bool, /* @__PURE__ */ P.special(P.variableName)],
    color: Zc
  },
  {
    tag: [P.processingInstruction, P.string, P.inserted],
    color: W1
  },
  {
    tag: P.invalid,
    color: N1
  }
]), Dw = [z1, /* @__PURE__ */ dd($1)];
class Ts {
  /**
  @internal
  */
  constructor(e, t, i, r, s, o, l, a, f, u = 0, g) {
    this.p = e, this.stack = t, this.state = i, this.reducePos = r, this.pos = s, this.score = o, this.buffer = l, this.bufferBase = a, this.curContext = f, this.lookAhead = u, this.parent = g;
  }
  /**
  @internal
  */
  toString() {
    return `[${this.stack.filter((e, t) => t % 3 == 0).concat(this.state)}]@${this.pos}${this.score ? "!" + this.score : ""}`;
  }
  // Start an empty stack
  /**
  @internal
  */
  static start(e, t, i = 0) {
    let r = e.parser.context;
    return new Ts(e, [], t, i, i, 0, [], 0, r ? new nf(r, r.start) : null, 0, null);
  }
  /**
  The stack's current [context](#lr.ContextTracker) value, if
  any. Its type will depend on the context tracker's type
  parameter, or it will be `null` if there is no context
  tracker.
  */
  get context() {
    return this.curContext ? this.curContext.context : null;
  }
  // Push a state onto the stack, tracking its start position as well
  // as the buffer base at that point.
  /**
  @internal
  */
  pushState(e, t) {
    this.stack.push(this.state, t, this.bufferBase + this.buffer.length), this.state = e;
  }
  // Apply a reduce action
  /**
  @internal
  */
  reduce(e) {
    var t;
    let i = e >> 19, r = e & 65535, { parser: s } = this.p, o = this.reducePos < this.pos - 25 && this.setLookAhead(this.pos), l = s.dynamicPrecedence(r);
    if (l && (this.score += l), i == 0) {
      this.pushState(s.getGoto(this.state, r, !0), this.reducePos), r < s.minRepeatTerm && this.storeNode(r, this.reducePos, this.reducePos, o ? 8 : 4, !0), this.reduceContext(r, this.reducePos);
      return;
    }
    let a = this.stack.length - (i - 1) * 3 - (e & 262144 ? 6 : 0), f = a ? this.stack[a - 2] : this.p.ranges[0].from, u = this.reducePos - f;
    u >= 2e3 && !(!((t = this.p.parser.nodeSet.types[r]) === null || t === void 0) && t.isAnonymous) && (f == this.p.lastBigReductionStart ? (this.p.bigReductionCount++, this.p.lastBigReductionSize = u) : this.p.lastBigReductionSize < u && (this.p.bigReductionCount = 1, this.p.lastBigReductionStart = f, this.p.lastBigReductionSize = u));
    let g = a ? this.stack[a - 1] : 0, y = this.bufferBase + this.buffer.length - g;
    if (r < s.minRepeatTerm || e & 131072) {
      let b = s.stateFlag(
        this.state,
        1
        /* StateFlag.Skipped */
      ) ? this.pos : this.reducePos;
      this.storeNode(r, f, b, y + 4, !0);
    }
    if (e & 262144)
      this.state = this.stack[a];
    else {
      let b = this.stack[a - 3];
      this.state = s.getGoto(b, r, !0);
    }
    for (; this.stack.length > a; )
      this.stack.pop();
    this.reduceContext(r, f);
  }
  // Shift a value into the buffer
  /**
  @internal
  */
  storeNode(e, t, i, r = 4, s = !1) {
    if (e == 0 && (!this.stack.length || this.stack[this.stack.length - 1] < this.buffer.length + this.bufferBase)) {
      let o = this, l = this.buffer.length;
      if (l == 0 && o.parent && (l = o.bufferBase - o.parent.bufferBase, o = o.parent), l > 0 && o.buffer[l - 4] == 0 && o.buffer[l - 1] > -1) {
        if (t == i)
          return;
        if (o.buffer[l - 2] >= t) {
          o.buffer[l - 2] = i;
          return;
        }
      }
    }
    if (!s || this.pos == i)
      this.buffer.push(e, t, i, r);
    else {
      let o = this.buffer.length;
      if (o > 0 && (this.buffer[o - 4] != 0 || this.buffer[o - 1] < 0)) {
        let l = !1;
        for (let a = o; a > 0 && this.buffer[a - 2] > i; a -= 4)
          if (this.buffer[a - 1] >= 0) {
            l = !0;
            break;
          }
        if (l)
          for (; o > 0 && this.buffer[o - 2] > i; )
            this.buffer[o] = this.buffer[o - 4], this.buffer[o + 1] = this.buffer[o - 3], this.buffer[o + 2] = this.buffer[o - 2], this.buffer[o + 3] = this.buffer[o - 1], o -= 4, r > 4 && (r -= 4);
      }
      this.buffer[o] = e, this.buffer[o + 1] = t, this.buffer[o + 2] = i, this.buffer[o + 3] = r;
    }
  }
  // Apply a shift action
  /**
  @internal
  */
  shift(e, t, i, r) {
    if (e & 131072)
      this.pushState(e & 65535, this.pos);
    else if ((e & 262144) == 0) {
      let s = e, { parser: o } = this.p;
      this.pos = r, !o.stateFlag(
        s,
        1
        /* StateFlag.Skipped */
      ) && (r > i || t <= o.maxNode) && (this.reducePos = r), this.pushState(s, Math.min(i, this.reducePos)), this.shiftContext(t, i), t <= o.maxNode && this.buffer.push(t, i, r, 4);
    } else
      this.pos = r, this.shiftContext(t, i), t <= this.p.parser.maxNode && this.buffer.push(t, i, r, 4);
  }
  // Apply an action
  /**
  @internal
  */
  apply(e, t, i, r) {
    e & 65536 ? this.reduce(e) : this.shift(e, t, i, r);
  }
  // Add a prebuilt (reused) node into the buffer.
  /**
  @internal
  */
  useNode(e, t) {
    let i = this.p.reused.length - 1;
    (i < 0 || this.p.reused[i] != e) && (this.p.reused.push(e), i++);
    let r = this.pos;
    this.reducePos = this.pos = r + e.length, this.pushState(t, r), this.buffer.push(
      i,
      r,
      this.reducePos,
      -1
      /* size == -1 means this is a reused value */
    ), this.curContext && this.updateContext(this.curContext.tracker.reuse(this.curContext.context, e, this, this.p.stream.reset(this.pos - e.length)));
  }
  // Split the stack. Due to the buffer sharing and the fact
  // that `this.stack` tends to stay quite shallow, this isn't very
  // expensive.
  /**
  @internal
  */
  split() {
    let e = this, t = e.buffer.length;
    for (; t > 0 && e.buffer[t - 2] > e.reducePos; )
      t -= 4;
    let i = e.buffer.slice(t), r = e.bufferBase + t;
    for (; e && r == e.bufferBase; )
      e = e.parent;
    return new Ts(this.p, this.stack.slice(), this.state, this.reducePos, this.pos, this.score, i, r, this.curContext, this.lookAhead, e);
  }
  // Try to recover from an error by 'deleting' (ignoring) one token.
  /**
  @internal
  */
  recoverByDelete(e, t) {
    let i = e <= this.p.parser.maxNode;
    i && this.storeNode(e, this.pos, t, 4), this.storeNode(0, this.pos, t, i ? 8 : 4), this.pos = this.reducePos = t, this.score -= 190;
  }
  /**
  Check if the given term would be able to be shifted (optionally
  after some reductions) on this stack. This can be useful for
  external tokenizers that want to make sure they only provide a
  given token when it applies.
  */
  canShift(e) {
    for (let t = new q1(this); ; ) {
      let i = this.p.parser.stateSlot(
        t.state,
        4
        /* ParseState.DefaultReduce */
      ) || this.p.parser.hasAction(t.state, e);
      if (i == 0)
        return !1;
      if ((i & 65536) == 0)
        return !0;
      t.reduce(i);
    }
  }
  // Apply up to Recover.MaxNext recovery actions that conceptually
  // inserts some missing token or rule.
  /**
  @internal
  */
  recoverByInsert(e) {
    if (this.stack.length >= 300)
      return [];
    let t = this.p.parser.nextStates(this.state);
    if (t.length > 8 || this.stack.length >= 120) {
      let r = [];
      for (let s = 0, o; s < t.length; s += 2)
        (o = t[s + 1]) != this.state && this.p.parser.hasAction(o, e) && r.push(t[s], o);
      if (this.stack.length < 120)
        for (let s = 0; r.length < 8 && s < t.length; s += 2) {
          let o = t[s + 1];
          r.some((l, a) => a & 1 && l == o) || r.push(t[s], o);
        }
      t = r;
    }
    let i = [];
    for (let r = 0; r < t.length && i.length < 4; r += 2) {
      let s = t[r + 1];
      if (s == this.state)
        continue;
      let o = this.split();
      o.pushState(s, this.pos), o.storeNode(0, o.pos, o.pos, 4, !0), o.shiftContext(t[r], this.pos), o.reducePos = this.pos, o.score -= 200, i.push(o);
    }
    return i;
  }
  // Force a reduce, if possible. Return false if that can't
  // be done.
  /**
  @internal
  */
  forceReduce() {
    let { parser: e } = this.p, t = e.stateSlot(
      this.state,
      5
      /* ParseState.ForcedReduce */
    );
    if ((t & 65536) == 0)
      return !1;
    if (!e.validAction(this.state, t)) {
      let i = t >> 19, r = t & 65535, s = this.stack.length - i * 3;
      if (s < 0 || e.getGoto(this.stack[s], r, !1) < 0) {
        let o = this.findForcedReduction();
        if (o == null)
          return !1;
        t = o;
      }
      this.storeNode(0, this.pos, this.pos, 4, !0), this.score -= 100;
    }
    return this.reducePos = this.pos, this.reduce(t), !0;
  }
  /**
  Try to scan through the automaton to find some kind of reduction
  that can be applied. Used when the regular ForcedReduce field
  isn't a valid action. @internal
  */
  findForcedReduction() {
    let { parser: e } = this.p, t = [], i = (r, s) => {
      if (!t.includes(r))
        return t.push(r), e.allActions(r, (o) => {
          if (!(o & 393216)) if (o & 65536) {
            let l = (o >> 19) - s;
            if (l > 1) {
              let a = o & 65535, f = this.stack.length - l * 3;
              if (f >= 0 && e.getGoto(this.stack[f], a, !1) >= 0)
                return l << 19 | 65536 | a;
            }
          } else {
            let l = i(o, s + 1);
            if (l != null)
              return l;
          }
        });
    };
    return i(this.state, 0);
  }
  /**
  @internal
  */
  forceAll() {
    for (; !this.p.parser.stateFlag(
      this.state,
      2
      /* StateFlag.Accepting */
    ); )
      if (!this.forceReduce()) {
        this.storeNode(0, this.pos, this.pos, 4, !0);
        break;
      }
    return this;
  }
  /**
  Check whether this state has no further actions (assumed to be a direct descendant of the
  top state, since any other states must be able to continue
  somehow). @internal
  */
  get deadEnd() {
    if (this.stack.length != 3)
      return !1;
    let { parser: e } = this.p;
    return e.data[e.stateSlot(
      this.state,
      1
      /* ParseState.Actions */
    )] == 65535 && !e.stateSlot(
      this.state,
      4
      /* ParseState.DefaultReduce */
    );
  }
  /**
  Restart the stack (put it back in its start state). Only safe
  when this.stack.length == 3 (state is directly below the top
  state). @internal
  */
  restart() {
    this.storeNode(0, this.pos, this.pos, 4, !0), this.state = this.stack[0], this.stack.length = 0;
  }
  /**
  @internal
  */
  sameState(e) {
    if (this.state != e.state || this.stack.length != e.stack.length)
      return !1;
    for (let t = 0; t < this.stack.length; t += 3)
      if (this.stack[t] != e.stack[t])
        return !1;
    return !0;
  }
  /**
  Get the parser used by this stack.
  */
  get parser() {
    return this.p.parser;
  }
  /**
  Test whether a given dialect (by numeric ID, as exported from
  the terms file) is enabled.
  */
  dialectEnabled(e) {
    return this.p.parser.dialect.flags[e];
  }
  shiftContext(e, t) {
    this.curContext && this.updateContext(this.curContext.tracker.shift(this.curContext.context, e, this, this.p.stream.reset(t)));
  }
  reduceContext(e, t) {
    this.curContext && this.updateContext(this.curContext.tracker.reduce(this.curContext.context, e, this, this.p.stream.reset(t)));
  }
  /**
  @internal
  */
  emitContext() {
    let e = this.buffer.length - 1;
    (e < 0 || this.buffer[e] != -3) && this.buffer.push(this.curContext.hash, this.pos, this.pos, -3);
  }
  /**
  @internal
  */
  emitLookAhead() {
    let e = this.buffer.length - 1;
    (e < 0 || this.buffer[e] != -4) && this.buffer.push(this.lookAhead, this.pos, this.pos, -4);
  }
  updateContext(e) {
    if (e != this.curContext.context) {
      let t = new nf(this.curContext.tracker, e);
      t.hash != this.curContext.hash && this.emitContext(), this.curContext = t;
    }
  }
  /**
  @internal
  */
  setLookAhead(e) {
    return e <= this.lookAhead ? !1 : (this.emitLookAhead(), this.lookAhead = e, !0);
  }
  /**
  @internal
  */
  close() {
    this.curContext && this.curContext.tracker.strict && this.emitContext(), this.lookAhead > 0 && this.emitLookAhead();
  }
}
class nf {
  constructor(e, t) {
    this.tracker = e, this.context = t, this.hash = e.strict ? e.hash(t) : 0;
  }
}
class q1 {
  constructor(e) {
    this.start = e, this.state = e.state, this.stack = e.stack, this.base = this.stack.length;
  }
  reduce(e) {
    let t = e & 65535, i = e >> 19;
    i == 0 ? (this.stack == this.start.stack && (this.stack = this.stack.slice()), this.stack.push(this.state, 0, 0), this.base += 3) : this.base -= (i - 1) * 3;
    let r = this.start.p.parser.getGoto(this.stack[this.base - 3], t, !0);
    this.state = r;
  }
}
class Ps {
  constructor(e, t, i) {
    this.stack = e, this.pos = t, this.index = i, this.buffer = e.buffer, this.index == 0 && this.maybeNext();
  }
  static create(e, t = e.bufferBase + e.buffer.length) {
    return new Ps(e, t, t - e.bufferBase);
  }
  maybeNext() {
    let e = this.stack.parent;
    e != null && (this.index = this.stack.bufferBase - e.bufferBase, this.stack = e, this.buffer = e.buffer);
  }
  get id() {
    return this.buffer[this.index - 4];
  }
  get start() {
    return this.buffer[this.index - 3];
  }
  get end() {
    return this.buffer[this.index - 2];
  }
  get size() {
    return this.buffer[this.index - 1];
  }
  next() {
    this.index -= 4, this.pos -= 4, this.index == 0 && this.maybeNext();
  }
  fork() {
    return new Ps(this.stack, this.pos, this.index);
  }
}
function qr(n, e = Uint16Array) {
  if (typeof n != "string")
    return n;
  let t = null;
  for (let i = 0, r = 0; i < n.length; ) {
    let s = 0;
    for (; ; ) {
      let o = n.charCodeAt(i++), l = !1;
      if (o == 126) {
        s = 65535;
        break;
      }
      o >= 92 && o--, o >= 34 && o--;
      let a = o - 32;
      if (a >= 46 && (a -= 46, l = !0), s += a, l)
        break;
      s *= 46;
    }
    t ? t[r++] = s : t = new e(s);
  }
  return t;
}
class es {
  constructor() {
    this.start = -1, this.value = -1, this.end = -1, this.extended = -1, this.lookAhead = 0, this.mask = 0, this.context = 0;
  }
}
const rf = new es();
class K1 {
  /**
  @internal
  */
  constructor(e, t) {
    this.input = e, this.ranges = t, this.chunk = "", this.chunkOff = 0, this.chunk2 = "", this.chunk2Pos = 0, this.next = -1, this.token = rf, this.rangeIndex = 0, this.pos = this.chunkPos = t[0].from, this.range = t[0], this.end = t[t.length - 1].to, this.readNext();
  }
  /**
  @internal
  */
  resolveOffset(e, t) {
    let i = this.range, r = this.rangeIndex, s = this.pos + e;
    for (; s < i.from; ) {
      if (!r)
        return null;
      let o = this.ranges[--r];
      s -= i.from - o.to, i = o;
    }
    for (; t < 0 ? s > i.to : s >= i.to; ) {
      if (r == this.ranges.length - 1)
        return null;
      let o = this.ranges[++r];
      s += o.from - i.to, i = o;
    }
    return s;
  }
  /**
  @internal
  */
  clipPos(e) {
    if (e >= this.range.from && e < this.range.to)
      return e;
    for (let t of this.ranges)
      if (t.to > e)
        return Math.max(e, t.from);
    return this.end;
  }
  /**
  Look at a code unit near the stream position. `.peek(0)` equals
  `.next`, `.peek(-1)` gives you the previous character, and so
  on.
  
  Note that looking around during tokenizing creates dependencies
  on potentially far-away content, which may reduce the
  effectiveness incremental parsing—when looking forward—or even
  cause invalid reparses when looking backward more than 25 code
  units, since the library does not track lookbehind.
  */
  peek(e) {
    let t = this.chunkOff + e, i, r;
    if (t >= 0 && t < this.chunk.length)
      i = this.pos + e, r = this.chunk.charCodeAt(t);
    else {
      let s = this.resolveOffset(e, 1);
      if (s == null)
        return -1;
      if (i = s, i >= this.chunk2Pos && i < this.chunk2Pos + this.chunk2.length)
        r = this.chunk2.charCodeAt(i - this.chunk2Pos);
      else {
        let o = this.rangeIndex, l = this.range;
        for (; l.to <= i; )
          l = this.ranges[++o];
        this.chunk2 = this.input.chunk(this.chunk2Pos = i), i + this.chunk2.length > l.to && (this.chunk2 = this.chunk2.slice(0, l.to - i)), r = this.chunk2.charCodeAt(0);
      }
    }
    return i >= this.token.lookAhead && (this.token.lookAhead = i + 1), r;
  }
  /**
  Accept a token. By default, the end of the token is set to the
  current stream position, but you can pass an offset (relative to
  the stream position) to change that.
  */
  acceptToken(e, t = 0) {
    let i = t ? this.resolveOffset(t, -1) : this.pos;
    if (i == null || i < this.token.start)
      throw new RangeError("Token end out of bounds");
    this.token.value = e, this.token.end = i;
  }
  /**
  Accept a token ending at a specific given position.
  */
  acceptTokenTo(e, t) {
    this.token.value = e, this.token.end = t;
  }
  getChunk() {
    if (this.pos >= this.chunk2Pos && this.pos < this.chunk2Pos + this.chunk2.length) {
      let { chunk: e, chunkPos: t } = this;
      this.chunk = this.chunk2, this.chunkPos = this.chunk2Pos, this.chunk2 = e, this.chunk2Pos = t, this.chunkOff = this.pos - this.chunkPos;
    } else {
      this.chunk2 = this.chunk, this.chunk2Pos = this.chunkPos;
      let e = this.input.chunk(this.pos), t = this.pos + e.length;
      this.chunk = t > this.range.to ? e.slice(0, this.range.to - this.pos) : e, this.chunkPos = this.pos, this.chunkOff = 0;
    }
  }
  readNext() {
    return this.chunkOff >= this.chunk.length && (this.getChunk(), this.chunkOff == this.chunk.length) ? this.next = -1 : this.next = this.chunk.charCodeAt(this.chunkOff);
  }
  /**
  Move the stream forward N (defaults to 1) code units. Returns
  the new value of [`next`](#lr.InputStream.next).
  */
  advance(e = 1) {
    for (this.chunkOff += e; this.pos + e >= this.range.to; ) {
      if (this.rangeIndex == this.ranges.length - 1)
        return this.setDone();
      e -= this.range.to - this.pos, this.range = this.ranges[++this.rangeIndex], this.pos = this.range.from;
    }
    return this.pos += e, this.pos >= this.token.lookAhead && (this.token.lookAhead = this.pos + 1), this.readNext();
  }
  setDone() {
    return this.pos = this.chunkPos = this.end, this.range = this.ranges[this.rangeIndex = this.ranges.length - 1], this.chunk = "", this.next = -1;
  }
  /**
  @internal
  */
  reset(e, t) {
    if (t ? (this.token = t, t.start = e, t.lookAhead = e + 1, t.value = t.extended = -1) : this.token = rf, this.pos != e) {
      if (this.pos = e, e == this.end)
        return this.setDone(), this;
      for (; e < this.range.from; )
        this.range = this.ranges[--this.rangeIndex];
      for (; e >= this.range.to; )
        this.range = this.ranges[++this.rangeIndex];
      e >= this.chunkPos && e < this.chunkPos + this.chunk.length ? this.chunkOff = e - this.chunkPos : (this.chunk = "", this.chunkOff = 0), this.readNext();
    }
    return this;
  }
  /**
  @internal
  */
  read(e, t) {
    if (e >= this.chunkPos && t <= this.chunkPos + this.chunk.length)
      return this.chunk.slice(e - this.chunkPos, t - this.chunkPos);
    if (e >= this.chunk2Pos && t <= this.chunk2Pos + this.chunk2.length)
      return this.chunk2.slice(e - this.chunk2Pos, t - this.chunk2Pos);
    if (e >= this.range.from && t <= this.range.to)
      return this.input.read(e, t);
    let i = "";
    for (let r of this.ranges) {
      if (r.from >= t)
        break;
      r.to > e && (i += this.input.read(Math.max(r.from, e), Math.min(r.to, t)));
    }
    return i;
  }
}
class mn {
  constructor(e, t) {
    this.data = e, this.id = t;
  }
  token(e, t) {
    let { parser: i } = t.p;
    _1(this.data, e, t, this.id, i.data, i.tokenPrecTable);
  }
}
mn.prototype.contextual = mn.prototype.fallback = mn.prototype.extend = !1;
mn.prototype.fallback = mn.prototype.extend = !1;
class vp {
  /**
  Create a tokenizer. The first argument is the function that,
  given an input stream, scans for the types of tokens it
  recognizes at the stream's position, and calls
  [`acceptToken`](#lr.InputStream.acceptToken) when it finds
  one.
  */
  constructor(e, t = {}) {
    this.token = e, this.contextual = !!t.contextual, this.fallback = !!t.fallback, this.extend = !!t.extend;
  }
}
function _1(n, e, t, i, r, s) {
  let o = 0, l = 1 << i, { dialect: a } = t.p.parser;
  e: for (; (l & n[o]) != 0; ) {
    let f = n[o + 1];
    for (let b = o + 3; b < f; b += 2)
      if ((n[b + 1] & l) > 0) {
        let k = n[b];
        if (a.allows(k) && (e.token.value == -1 || e.token.value == k || j1(k, e.token.value, r, s))) {
          e.acceptToken(k);
          break;
        }
      }
    let u = e.next, g = 0, y = n[o + 2];
    if (e.next < 0 && y > g && n[f + y * 3 - 3] == 65535) {
      o = n[f + y * 3 - 1];
      continue e;
    }
    for (; g < y; ) {
      let b = g + y >> 1, k = f + b + (b << 1), C = n[k], M = n[k + 1] || 65536;
      if (u < C)
        y = b;
      else if (u >= M)
        g = b + 1;
      else {
        o = n[k + 2], e.advance();
        continue e;
      }
    }
    break;
  }
}
function sf(n, e, t) {
  for (let i = e, r; (r = n[i]) != 65535; i++)
    if (r == t)
      return i - e;
  return -1;
}
function j1(n, e, t, i) {
  let r = sf(t, i, e);
  return r < 0 || sf(t, i, n) < r;
}
const gt = typeof process < "u" && process.env && /\bparse\b/.test(process.env.LOG);
let No = null;
function of(n, e, t) {
  let i = n.cursor(be.IncludeAnonymous);
  for (i.moveTo(e); ; )
    if (!(t < 0 ? i.childBefore(e) : i.childAfter(e)))
      for (; ; ) {
        if ((t < 0 ? i.to < e : i.from > e) && !i.type.isError)
          return t < 0 ? Math.max(0, Math.min(
            i.to - 1,
            e - 25
            /* Lookahead.Margin */
          )) : Math.min(n.length, Math.max(
            i.from + 1,
            e + 25
            /* Lookahead.Margin */
          ));
        if (t < 0 ? i.prevSibling() : i.nextSibling())
          break;
        if (!i.parent())
          return t < 0 ? 0 : n.length;
      }
}
class U1 {
  constructor(e, t) {
    this.fragments = e, this.nodeSet = t, this.i = 0, this.fragment = null, this.safeFrom = -1, this.safeTo = -1, this.trees = [], this.start = [], this.index = [], this.nextFragment();
  }
  nextFragment() {
    let e = this.fragment = this.i == this.fragments.length ? null : this.fragments[this.i++];
    if (e) {
      for (this.safeFrom = e.openStart ? of(e.tree, e.from + e.offset, 1) - e.offset : e.from, this.safeTo = e.openEnd ? of(e.tree, e.to + e.offset, -1) - e.offset : e.to; this.trees.length; )
        this.trees.pop(), this.start.pop(), this.index.pop();
      this.trees.push(e.tree), this.start.push(-e.offset), this.index.push(0), this.nextStart = this.safeFrom;
    } else
      this.nextStart = 1e9;
  }
  // `pos` must be >= any previously given `pos` for this cursor
  nodeAt(e) {
    if (e < this.nextStart)
      return null;
    for (; this.fragment && this.safeTo <= e; )
      this.nextFragment();
    if (!this.fragment)
      return null;
    for (; ; ) {
      let t = this.trees.length - 1;
      if (t < 0)
        return this.nextFragment(), null;
      let i = this.trees[t], r = this.index[t];
      if (r == i.children.length) {
        this.trees.pop(), this.start.pop(), this.index.pop();
        continue;
      }
      let s = i.children[r], o = this.start[t] + i.positions[r];
      if (o > e)
        return this.nextStart = o, null;
      if (s instanceof Te) {
        if (o == e) {
          if (o < this.safeFrom)
            return null;
          let l = o + s.length;
          if (l <= this.safeTo) {
            let a = s.prop(oe.lookAhead);
            if (!a || l + a < this.fragment.to)
              return s;
          }
        }
        this.index[t]++, o + s.length >= Math.max(this.safeFrom, e) && (this.trees.push(s), this.start.push(o), this.index.push(0));
      } else
        this.index[t]++, this.nextStart = o + s.length;
    }
  }
}
class X1 {
  constructor(e, t) {
    this.stream = t, this.tokens = [], this.mainToken = null, this.actions = [], this.tokens = e.tokenizers.map((i) => new es());
  }
  getActions(e) {
    let t = 0, i = null, { parser: r } = e.p, { tokenizers: s } = r, o = r.stateSlot(
      e.state,
      3
      /* ParseState.TokenizerMask */
    ), l = e.curContext ? e.curContext.hash : 0, a = 0;
    for (let f = 0; f < s.length; f++) {
      if ((1 << f & o) == 0)
        continue;
      let u = s[f], g = this.tokens[f];
      if (!(i && !u.fallback) && ((u.contextual || g.start != e.pos || g.mask != o || g.context != l) && (this.updateCachedToken(g, u, e), g.mask = o, g.context = l), g.lookAhead > g.end + 25 && (a = Math.max(g.lookAhead, a)), g.value != 0)) {
        let y = t;
        if (g.extended > -1 && (t = this.addActions(e, g.extended, g.end, t)), t = this.addActions(e, g.value, g.end, t), !u.extend && (i = g, t > y))
          break;
      }
    }
    for (; this.actions.length > t; )
      this.actions.pop();
    return a && e.setLookAhead(a), !i && e.pos == this.stream.end && (i = new es(), i.value = e.p.parser.eofTerm, i.start = i.end = e.pos, t = this.addActions(e, i.value, i.end, t)), this.mainToken = i, this.actions;
  }
  getMainToken(e) {
    if (this.mainToken)
      return this.mainToken;
    let t = new es(), { pos: i, p: r } = e;
    return t.start = i, t.end = Math.min(i + 1, r.stream.end), t.value = i == r.stream.end ? r.parser.eofTerm : 0, t;
  }
  updateCachedToken(e, t, i) {
    let r = this.stream.clipPos(i.pos);
    if (t.token(this.stream.reset(r, e), i), e.value > -1) {
      let { parser: s } = i.p;
      for (let o = 0; o < s.specialized.length; o++)
        if (s.specialized[o] == e.value) {
          let l = s.specializers[o](this.stream.read(e.start, e.end), i);
          if (l >= 0 && i.p.parser.dialect.allows(l >> 1)) {
            (l & 1) == 0 ? e.value = l >> 1 : e.extended = l >> 1;
            break;
          }
        }
    } else
      e.value = 0, e.end = this.stream.clipPos(r + 1);
  }
  putAction(e, t, i, r) {
    for (let s = 0; s < r; s += 3)
      if (this.actions[s] == e)
        return r;
    return this.actions[r++] = e, this.actions[r++] = t, this.actions[r++] = i, r;
  }
  addActions(e, t, i, r) {
    let { state: s } = e, { parser: o } = e.p, { data: l } = o;
    for (let a = 0; a < 2; a++)
      for (let f = o.stateSlot(
        s,
        a ? 2 : 1
        /* ParseState.Actions */
      ); ; f += 3) {
        if (l[f] == 65535)
          if (l[f + 1] == 1)
            f = ni(l, f + 2);
          else {
            r == 0 && l[f + 1] == 2 && (r = this.putAction(ni(l, f + 2), t, i, r));
            break;
          }
        l[f] == t && (r = this.putAction(ni(l, f + 1), t, i, r));
      }
    return r;
  }
}
class Y1 {
  constructor(e, t, i, r) {
    this.parser = e, this.input = t, this.ranges = r, this.recovering = 0, this.nextStackID = 9812, this.minStackPos = 0, this.reused = [], this.stoppedAt = null, this.lastBigReductionStart = -1, this.lastBigReductionSize = 0, this.bigReductionCount = 0, this.stream = new K1(t, r), this.tokens = new X1(e, this.stream), this.topTerm = e.top[1];
    let { from: s } = r[0];
    this.stacks = [Ts.start(this, e.top[0], s)], this.fragments = i.length && this.stream.end - s > e.bufferLength * 4 ? new U1(i, e.nodeSet) : null;
  }
  get parsedPos() {
    return this.minStackPos;
  }
  // Move the parser forward. This will process all parse stacks at
  // `this.pos` and try to advance them to a further position. If no
  // stack for such a position is found, it'll start error-recovery.
  //
  // When the parse is finished, this will return a syntax tree. When
  // not, it returns `null`.
  advance() {
    let e = this.stacks, t = this.minStackPos, i = this.stacks = [], r, s;
    if (this.bigReductionCount > 300 && e.length == 1) {
      let [o] = e;
      for (; o.forceReduce() && o.stack.length && o.stack[o.stack.length - 2] >= this.lastBigReductionStart; )
        ;
      this.bigReductionCount = this.lastBigReductionSize = 0;
    }
    for (let o = 0; o < e.length; o++) {
      let l = e[o];
      for (; ; ) {
        if (this.tokens.mainToken = null, l.pos > t)
          i.push(l);
        else {
          if (this.advanceStack(l, i, e))
            continue;
          {
            r || (r = [], s = []), r.push(l);
            let a = this.tokens.getMainToken(l);
            s.push(a.value, a.end);
          }
        }
        break;
      }
    }
    if (!i.length) {
      let o = r && Z1(r);
      if (o)
        return gt && console.log("Finish with " + this.stackID(o)), this.stackToTree(o);
      if (this.parser.strict)
        throw gt && r && console.log("Stuck with token " + (this.tokens.mainToken ? this.parser.getName(this.tokens.mainToken.value) : "none")), new SyntaxError("No parse at " + t);
      this.recovering || (this.recovering = 5);
    }
    if (this.recovering && r) {
      let o = this.stoppedAt != null && r[0].pos > this.stoppedAt ? r[0] : this.runRecovery(r, s, i);
      if (o)
        return gt && console.log("Force-finish " + this.stackID(o)), this.stackToTree(o.forceAll());
    }
    if (this.recovering) {
      let o = this.recovering == 1 ? 1 : this.recovering * 3;
      if (i.length > o)
        for (i.sort((l, a) => a.score - l.score); i.length > o; )
          i.pop();
      i.some((l) => l.reducePos > t) && this.recovering--;
    } else if (i.length > 1) {
      e: for (let o = 0; o < i.length - 1; o++) {
        let l = i[o];
        for (let a = o + 1; a < i.length; a++) {
          let f = i[a];
          if (l.sameState(f) || l.buffer.length > 500 && f.buffer.length > 500)
            if ((l.score - f.score || l.buffer.length - f.buffer.length) > 0)
              i.splice(a--, 1);
            else {
              i.splice(o--, 1);
              continue e;
            }
        }
      }
      i.length > 12 && (i.sort((o, l) => l.score - o.score), i.splice(
        12,
        i.length - 12
        /* Rec.MaxStackCount */
      ));
    }
    this.minStackPos = i[0].pos;
    for (let o = 1; o < i.length; o++)
      i[o].pos < this.minStackPos && (this.minStackPos = i[o].pos);
    return null;
  }
  stopAt(e) {
    if (this.stoppedAt != null && this.stoppedAt < e)
      throw new RangeError("Can't move stoppedAt forward");
    this.stoppedAt = e;
  }
  // Returns an updated version of the given stack, or null if the
  // stack can't advance normally. When `split` and `stacks` are
  // given, stacks split off by ambiguous operations will be pushed to
  // `split`, or added to `stacks` if they move `pos` forward.
  advanceStack(e, t, i) {
    let r = e.pos, { parser: s } = this, o = gt ? this.stackID(e) + " -> " : "";
    if (this.stoppedAt != null && r > this.stoppedAt)
      return e.forceReduce() ? e : null;
    if (this.fragments) {
      let f = e.curContext && e.curContext.tracker.strict, u = f ? e.curContext.hash : 0;
      for (let g = this.fragments.nodeAt(r); g; ) {
        let y = this.parser.nodeSet.types[g.type.id] == g.type ? s.getGoto(e.state, g.type.id) : -1;
        if (y > -1 && g.length && (!f || (g.prop(oe.contextHash) || 0) == u))
          return e.useNode(g, y), gt && console.log(o + this.stackID(e) + ` (via reuse of ${s.getName(g.type.id)})`), !0;
        if (!(g instanceof Te) || g.children.length == 0 || g.positions[0] > 0)
          break;
        let b = g.children[0];
        if (b instanceof Te && g.positions[0] == 0)
          g = b;
        else
          break;
      }
    }
    let l = s.stateSlot(
      e.state,
      4
      /* ParseState.DefaultReduce */
    );
    if (l > 0)
      return e.reduce(l), gt && console.log(o + this.stackID(e) + ` (via always-reduce ${s.getName(
        l & 65535
        /* Action.ValueMask */
      )})`), !0;
    if (e.stack.length >= 8400)
      for (; e.stack.length > 6e3 && e.forceReduce(); )
        ;
    let a = this.tokens.getActions(e);
    for (let f = 0; f < a.length; ) {
      let u = a[f++], g = a[f++], y = a[f++], b = f == a.length || !i, k = b ? e : e.split(), C = this.tokens.mainToken;
      if (k.apply(u, g, C ? C.start : k.pos, y), gt && console.log(o + this.stackID(k) + ` (via ${(u & 65536) == 0 ? "shift" : `reduce of ${s.getName(
        u & 65535
        /* Action.ValueMask */
      )}`} for ${s.getName(g)} @ ${r}${k == e ? "" : ", split"})`), b)
        return !0;
      k.pos > r ? t.push(k) : i.push(k);
    }
    return !1;
  }
  // Advance a given stack forward as far as it will go. Returns the
  // (possibly updated) stack if it got stuck, or null if it moved
  // forward and was given to `pushStackDedup`.
  advanceFully(e, t) {
    let i = e.pos;
    for (; ; ) {
      if (!this.advanceStack(e, null, null))
        return !1;
      if (e.pos > i)
        return lf(e, t), !0;
    }
  }
  runRecovery(e, t, i) {
    let r = null, s = !1;
    for (let o = 0; o < e.length; o++) {
      let l = e[o], a = t[o << 1], f = t[(o << 1) + 1], u = gt ? this.stackID(l) + " -> " : "";
      if (l.deadEnd && (s || (s = !0, l.restart(), gt && console.log(u + this.stackID(l) + " (restarted)"), this.advanceFully(l, i))))
        continue;
      let g = l.split(), y = u;
      for (let b = 0; b < 10 && g.forceReduce() && (gt && console.log(y + this.stackID(g) + " (via force-reduce)"), !this.advanceFully(g, i)); b++)
        gt && (y = this.stackID(g) + " -> ");
      for (let b of l.recoverByInsert(a))
        gt && console.log(u + this.stackID(b) + " (via recover-insert)"), this.advanceFully(b, i);
      this.stream.end > l.pos ? (f == l.pos && (f++, a = 0), l.recoverByDelete(a, f), gt && console.log(u + this.stackID(l) + ` (via recover-delete ${this.parser.getName(a)})`), lf(l, i)) : (!r || r.score < g.score) && (r = g);
    }
    return r;
  }
  // Convert the stack's buffer to a syntax tree.
  stackToTree(e) {
    return e.close(), Te.build({
      buffer: Ps.create(e),
      nodeSet: this.parser.nodeSet,
      topID: this.topTerm,
      maxBufferLength: this.parser.bufferLength,
      reused: this.reused,
      start: this.ranges[0].from,
      length: e.pos - this.ranges[0].from,
      minRepeatType: this.parser.minRepeatTerm
    });
  }
  stackID(e) {
    let t = (No || (No = /* @__PURE__ */ new WeakMap())).get(e);
    return t || No.set(e, t = String.fromCodePoint(this.nextStackID++)), t + e;
  }
}
function lf(n, e) {
  for (let t = 0; t < e.length; t++) {
    let i = e[t];
    if (i.pos == n.pos && i.sameState(n)) {
      e[t].score < n.score && (e[t] = n);
      return;
    }
  }
  e.push(n);
}
class G1 {
  constructor(e, t, i) {
    this.source = e, this.flags = t, this.disabled = i;
  }
  allows(e) {
    return !this.disabled || this.disabled[e] == 0;
  }
}
class An extends Gu {
  /**
  @internal
  */
  constructor(e) {
    if (super(), this.wrappers = [], e.version != 14)
      throw new RangeError(`Parser version (${e.version}) doesn't match runtime version (14)`);
    let t = e.nodeNames.split(" ");
    this.minRepeatTerm = t.length;
    for (let l = 0; l < e.repeatNodeCount; l++)
      t.push("");
    let i = Object.keys(e.topRules).map((l) => e.topRules[l][1]), r = [];
    for (let l = 0; l < t.length; l++)
      r.push([]);
    function s(l, a, f) {
      r[l].push([a, a.deserialize(String(f))]);
    }
    if (e.nodeProps)
      for (let l of e.nodeProps) {
        let a = l[0];
        typeof a == "string" && (a = oe[a]);
        for (let f = 1; f < l.length; ) {
          let u = l[f++];
          if (u >= 0)
            s(u, a, l[f++]);
          else {
            let g = l[f + -u];
            for (let y = -u; y > 0; y--)
              s(l[f++], a, g);
            f++;
          }
        }
      }
    this.nodeSet = new sa(t.map((l, a) => ot.define({
      name: a >= this.minRepeatTerm ? void 0 : l,
      id: a,
      props: r[a],
      top: i.indexOf(a) > -1,
      error: a == 0,
      skipped: e.skippedNodes && e.skippedNodes.indexOf(a) > -1
    }))), e.propSources && (this.nodeSet = this.nodeSet.extend(...e.propSources)), this.strict = !1, this.bufferLength = ju;
    let o = qr(e.tokenData);
    this.context = e.context, this.specializerSpecs = e.specialized || [], this.specialized = new Uint16Array(this.specializerSpecs.length);
    for (let l = 0; l < this.specializerSpecs.length; l++)
      this.specialized[l] = this.specializerSpecs[l].term;
    this.specializers = this.specializerSpecs.map(af), this.states = qr(e.states, Uint32Array), this.data = qr(e.stateData), this.goto = qr(e.goto), this.maxTerm = e.maxTerm, this.tokenizers = e.tokenizers.map((l) => typeof l == "number" ? new mn(o, l) : l), this.topRules = e.topRules, this.dialects = e.dialects || {}, this.dynamicPrecedences = e.dynamicPrecedences || null, this.tokenPrecTable = e.tokenPrec, this.termNames = e.termNames || null, this.maxNode = this.nodeSet.types.length - 1, this.dialect = this.parseDialect(), this.top = this.topRules[Object.keys(this.topRules)[0]];
  }
  createParse(e, t, i) {
    let r = new Y1(this, e, t, i);
    for (let s of this.wrappers)
      r = s(r, e, t, i);
    return r;
  }
  /**
  Get a goto table entry @internal
  */
  getGoto(e, t, i = !1) {
    let r = this.goto;
    if (t >= r[0])
      return -1;
    for (let s = r[t + 1]; ; ) {
      let o = r[s++], l = o & 1, a = r[s++];
      if (l && i)
        return a;
      for (let f = s + (o >> 1); s < f; s++)
        if (r[s] == e)
          return a;
      if (l)
        return -1;
    }
  }
  /**
  Check if this state has an action for a given terminal @internal
  */
  hasAction(e, t) {
    let i = this.data;
    for (let r = 0; r < 2; r++)
      for (let s = this.stateSlot(
        e,
        r ? 2 : 1
        /* ParseState.Actions */
      ), o; ; s += 3) {
        if ((o = i[s]) == 65535)
          if (i[s + 1] == 1)
            o = i[s = ni(i, s + 2)];
          else {
            if (i[s + 1] == 2)
              return ni(i, s + 2);
            break;
          }
        if (o == t || o == 0)
          return ni(i, s + 1);
      }
    return 0;
  }
  /**
  @internal
  */
  stateSlot(e, t) {
    return this.states[e * 6 + t];
  }
  /**
  @internal
  */
  stateFlag(e, t) {
    return (this.stateSlot(
      e,
      0
      /* ParseState.Flags */
    ) & t) > 0;
  }
  /**
  @internal
  */
  validAction(e, t) {
    return !!this.allActions(e, (i) => i == t ? !0 : null);
  }
  /**
  @internal
  */
  allActions(e, t) {
    let i = this.stateSlot(
      e,
      4
      /* ParseState.DefaultReduce */
    ), r = i ? t(i) : void 0;
    for (let s = this.stateSlot(
      e,
      1
      /* ParseState.Actions */
    ); r == null; s += 3) {
      if (this.data[s] == 65535)
        if (this.data[s + 1] == 1)
          s = ni(this.data, s + 2);
        else
          break;
      r = t(ni(this.data, s + 1));
    }
    return r;
  }
  /**
  Get the states that can follow this one through shift actions or
  goto jumps. @internal
  */
  nextStates(e) {
    let t = [];
    for (let i = this.stateSlot(
      e,
      1
      /* ParseState.Actions */
    ); ; i += 3) {
      if (this.data[i] == 65535)
        if (this.data[i + 1] == 1)
          i = ni(this.data, i + 2);
        else
          break;
      if ((this.data[i + 2] & 1) == 0) {
        let r = this.data[i + 1];
        t.some((s, o) => o & 1 && s == r) || t.push(this.data[i], r);
      }
    }
    return t;
  }
  /**
  Configure the parser. Returns a new parser instance that has the
  given settings modified. Settings not provided in `config` are
  kept from the original parser.
  */
  configure(e) {
    let t = Object.assign(Object.create(An.prototype), this);
    if (e.props && (t.nodeSet = this.nodeSet.extend(...e.props)), e.top) {
      let i = this.topRules[e.top];
      if (!i)
        throw new RangeError(`Invalid top rule name ${e.top}`);
      t.top = i;
    }
    return e.tokenizers && (t.tokenizers = this.tokenizers.map((i) => {
      let r = e.tokenizers.find((s) => s.from == i);
      return r ? r.to : i;
    })), e.specializers && (t.specializers = this.specializers.slice(), t.specializerSpecs = this.specializerSpecs.map((i, r) => {
      let s = e.specializers.find((l) => l.from == i.external);
      if (!s)
        return i;
      let o = Object.assign(Object.assign({}, i), { external: s.to });
      return t.specializers[r] = af(o), o;
    })), e.contextTracker && (t.context = e.contextTracker), e.dialect && (t.dialect = this.parseDialect(e.dialect)), e.strict != null && (t.strict = e.strict), e.wrap && (t.wrappers = t.wrappers.concat(e.wrap)), e.bufferLength != null && (t.bufferLength = e.bufferLength), t;
  }
  /**
  Tells you whether any [parse wrappers](#lr.ParserConfig.wrap)
  are registered for this parser.
  */
  hasWrappers() {
    return this.wrappers.length > 0;
  }
  /**
  Returns the name associated with a given term. This will only
  work for all terms when the parser was generated with the
  `--names` option. By default, only the names of tagged terms are
  stored.
  */
  getName(e) {
    return this.termNames ? this.termNames[e] : String(e <= this.maxNode && this.nodeSet.types[e].name || e);
  }
  /**
  The eof term id is always allocated directly after the node
  types. @internal
  */
  get eofTerm() {
    return this.maxNode + 1;
  }
  /**
  The type of top node produced by the parser.
  */
  get topNode() {
    return this.nodeSet.types[this.top[1]];
  }
  /**
  @internal
  */
  dynamicPrecedence(e) {
    let t = this.dynamicPrecedences;
    return t == null ? 0 : t[e] || 0;
  }
  /**
  @internal
  */
  parseDialect(e) {
    let t = Object.keys(this.dialects), i = t.map(() => !1);
    if (e)
      for (let s of e.split(" ")) {
        let o = t.indexOf(s);
        o >= 0 && (i[o] = !0);
      }
    let r = null;
    for (let s = 0; s < t.length; s++)
      if (!i[s])
        for (let o = this.dialects[t[s]], l; (l = this.data[o++]) != 65535; )
          (r || (r = new Uint8Array(this.maxTerm + 1)))[l] = 1;
    return new G1(e, i, r);
  }
  /**
  Used by the output of the parser generator. Not available to
  user code. @hide
  */
  static deserialize(e) {
    return new An(e);
  }
}
function ni(n, e) {
  return n[e] | n[e + 1] << 16;
}
function Z1(n) {
  let e = null;
  for (let t of n) {
    let i = t.p.stoppedAt;
    (t.pos == t.p.stream.end || i != null && t.pos > i) && t.p.parser.stateFlag(
      t.state,
      2
      /* StateFlag.Accepting */
    ) && (!e || e.score < t.score) && (e = t);
  }
  return e;
}
function af(n) {
  if (n.external) {
    let e = n.extend ? 1 : 0;
    return (t, i) => n.external(t, i) << 1 | e;
  }
  return n.get;
}
const J1 = Qs({
  String: P.string,
  Number: P.number,
  "True False": P.bool,
  PropertyName: P.propertyName,
  Null: P.null,
  ", :": P.separator,
  "[ ]": P.squareBracket,
  "{ }": P.brace
}), ek = An.deserialize({
  version: 14,
  states: "$bOVQPOOOOQO'#Cb'#CbOnQPO'#CeOvQPO'#ClOOQO'#Cr'#CrQOQPOOOOQO'#Cg'#CgO}QPO'#CfO!SQPO'#CtOOQO,59P,59PO![QPO,59PO!aQPO'#CuOOQO,59W,59WO!iQPO,59WOVQPO,59QOqQPO'#CmO!nQPO,59`OOQO1G.k1G.kOVQPO'#CnO!vQPO,59aOOQO1G.r1G.rOOQO1G.l1G.lOOQO,59X,59XOOQO-E6k-E6kOOQO,59Y,59YOOQO-E6l-E6l",
  stateData: "#O~OeOS~OQSORSOSSOTSOWQO_ROgPO~OVXOgUO~O^[O~PVO[^O~O]_OVhX~OVaO~O]bO^iX~O^dO~O]_OVha~O]bO^ia~O",
  goto: "!kjPPPPPPkPPkqwPPPPk{!RPPP!XP!e!hXSOR^bQWQRf_TVQ_Q`WRg`QcZRicQTOQZRQe^RhbRYQR]R",
  nodeNames: "⚠ JsonText True False Null Number String } { Object Property PropertyName : , ] [ Array",
  maxTerm: 25,
  nodeProps: [
    ["isolate", -2, 6, 11, ""],
    ["openedBy", 7, "{", 14, "["],
    ["closedBy", 8, "}", 15, "]"]
  ],
  propSources: [J1],
  skippedNodes: [0],
  repeatNodeCount: 2,
  tokenData: "(|~RaXY!WYZ!W]^!Wpq!Wrs!]|}$u}!O$z!Q!R%T!R![&c![!]&t!}#O&y#P#Q'O#Y#Z'T#b#c'r#h#i(Z#o#p(r#q#r(w~!]Oe~~!`Wpq!]qr!]rs!xs#O!]#O#P!}#P;'S!];'S;=`$o<%lO!]~!}Og~~#QXrs!]!P!Q!]#O#P!]#U#V!]#Y#Z!]#b#c!]#f#g!]#h#i!]#i#j#m~#pR!Q![#y!c!i#y#T#Z#y~#|R!Q![$V!c!i$V#T#Z$V~$YR!Q![$c!c!i$c#T#Z$c~$fR!Q![!]!c!i!]#T#Z!]~$rP;=`<%l!]~$zO]~~$}Q!Q!R%T!R![&c~%YRT~!O!P%c!g!h%w#X#Y%w~%fP!Q![%i~%nRT~!Q![%i!g!h%w#X#Y%w~%zR{|&T}!O&T!Q![&Z~&WP!Q![&Z~&`PT~!Q![&Z~&hST~!O!P%c!Q![&c!g!h%w#X#Y%w~&yO[~~'OO_~~'TO^~~'WP#T#U'Z~'^P#`#a'a~'dP#g#h'g~'jP#X#Y'm~'rOR~~'uP#i#j'x~'{P#`#a(O~(RP#`#a(U~(ZOS~~(^P#f#g(a~(dP#i#j(g~(jP#X#Y(m~(rOQ~~(wOW~~(|OV~",
  tokenizers: [0],
  topRules: { JsonText: [0, 1] },
  tokenPrec: 0
}), tk = /* @__PURE__ */ Ui.define({
  name: "json",
  parser: /* @__PURE__ */ ek.configure({
    props: [
      /* @__PURE__ */ Hs.add({
        Object: /* @__PURE__ */ rr({ except: /^\s*\}/ }),
        Array: /* @__PURE__ */ rr({ except: /^\s*\]/ })
      }),
      /* @__PURE__ */ zs.add({
        "Object Array": Sl
      })
    ]
  }),
  languageData: {
    closeBrackets: { brackets: ["[", "{", '"'] },
    indentOnInput: /^\s*[\}\]]$/
  }
});
function Bw() {
  return new ha(tk);
}
const ik = 36, hf = 1, nk = 2, tn = 3, Fo = 4, rk = 5, sk = 6, ok = 7, lk = 8, ak = 9, hk = 10, ck = 11, fk = 12, uk = 13, dk = 14, pk = 15, gk = 16, mk = 17, cf = 18, vk = 19, yp = 20, bp = 21, ff = 22, yk = 23, bk = 24;
function Bl(n) {
  return n >= 65 && n <= 90 || n >= 97 && n <= 122 || n >= 48 && n <= 57;
}
function xk(n) {
  return n >= 48 && n <= 57 || n >= 97 && n <= 102 || n >= 65 && n <= 70;
}
function Fi(n, e, t) {
  for (let i = !1; ; ) {
    if (n.next < 0)
      return;
    if (n.next == e && !i) {
      n.advance();
      return;
    }
    i = t && !i && n.next == 92, n.advance();
  }
}
function kk(n, e) {
  e: for (; ; ) {
    if (n.next < 0)
      return;
    if (n.next == 36) {
      n.advance();
      for (let t = 0; t < e.length; t++) {
        if (n.next != e.charCodeAt(t))
          continue e;
        n.advance();
      }
      if (n.next == 36) {
        n.advance();
        return;
      }
    } else
      n.advance();
  }
}
function wk(n, e) {
  let t = "[{<(".indexOf(String.fromCharCode(e)), i = t < 0 ? e : "]}>)".charCodeAt(t);
  for (; ; ) {
    if (n.next < 0)
      return;
    if (n.next == i && n.peek(1) == 39) {
      n.advance(2);
      return;
    }
    n.advance();
  }
}
function El(n, e) {
  for (; !(n.next != 95 && !Bl(n.next)); )
    e != null && (e += String.fromCharCode(n.next)), n.advance();
  return e;
}
function Sk(n) {
  if (n.next == 39 || n.next == 34 || n.next == 96) {
    let e = n.next;
    n.advance(), Fi(n, e, !1);
  } else
    El(n);
}
function uf(n, e) {
  for (; n.next == 48 || n.next == 49; )
    n.advance();
  e && n.next == e && n.advance();
}
function df(n, e) {
  for (; ; ) {
    if (n.next == 46) {
      if (e)
        break;
      e = !0;
    } else if (n.next < 48 || n.next > 57)
      break;
    n.advance();
  }
  if (n.next == 69 || n.next == 101)
    for (n.advance(), (n.next == 43 || n.next == 45) && n.advance(); n.next >= 48 && n.next <= 57; )
      n.advance();
}
function pf(n) {
  for (; !(n.next < 0 || n.next == 10); )
    n.advance();
}
function Bi(n, e) {
  for (let t = 0; t < e.length; t++)
    if (e.charCodeAt(t) == n)
      return !0;
  return !1;
}
const Wo = ` 	\r
`;
function xp(n, e, t) {
  let i = /* @__PURE__ */ Object.create(null);
  i.true = i.false = rk, i.null = i.unknown = sk;
  for (let r of n.split(" "))
    r && (i[r] = yp);
  for (let r of e.split(" "))
    r && (i[r] = bp);
  for (let r of (t || "").split(" "))
    r && (i[r] = bk);
  return i;
}
const Ck = "array binary bit boolean char character clob date decimal double float int integer interval large national nchar nclob numeric object precision real smallint time timestamp varchar varying ", Ok = "absolute action add after all allocate alter and any are as asc assertion at authorization before begin between both breadth by call cascade cascaded case cast catalog check close collate collation column commit condition connect connection constraint constraints constructor continue corresponding count create cross cube current current_date current_default_transform_group current_transform_group_for_type current_path current_role current_time current_timestamp current_user cursor cycle data day deallocate declare default deferrable deferred delete depth deref desc describe descriptor deterministic diagnostics disconnect distinct do domain drop dynamic each else elseif end end-exec equals escape except exception exec execute exists exit external fetch first for foreign found from free full function general get global go goto grant group grouping handle having hold hour identity if immediate in indicator initially inner inout input insert intersect into is isolation join key language last lateral leading leave left level like limit local localtime localtimestamp locator loop map match method minute modifies module month names natural nesting new next no none not of old on only open option or order ordinality out outer output overlaps pad parameter partial path prepare preserve primary prior privileges procedure public read reads recursive redo ref references referencing relative release repeat resignal restrict result return returns revoke right role rollback rollup routine row rows savepoint schema scroll search second section select session session_user set sets signal similar size some space specific specifictype sql sqlexception sqlstate sqlwarning start state static system_user table temporary then timezone_hour timezone_minute to trailing transaction translation treat trigger under undo union unique unnest until update usage user using value values view when whenever where while with without work write year zone ", Il = {
  backslashEscapes: !1,
  hashComments: !1,
  spaceAfterDashes: !1,
  slashComments: !1,
  doubleQuotedStrings: !1,
  doubleDollarQuotedStrings: !1,
  unquotedBitLiterals: !1,
  treatBitsAsBytes: !1,
  charSetCasts: !1,
  plsqlQuotingMechanism: !1,
  operatorChars: "*+-%<>!=&|~^/",
  specialVar: "?",
  identifierQuotes: '"',
  caseInsensitiveIdentifiers: !1,
  words: /* @__PURE__ */ xp(Ok, Ck)
};
function Ak(n, e, t, i) {
  let r = {};
  for (let s in Il)
    r[s] = (n.hasOwnProperty(s) ? n : Il)[s];
  return e && (r.words = xp(e, t || "", i)), r;
}
function kp(n) {
  return new vp((e) => {
    var t;
    let { next: i } = e;
    if (e.advance(), Bi(i, Wo)) {
      for (; Bi(e.next, Wo); )
        e.advance();
      e.acceptToken(ik);
    } else if (i == 36 && n.doubleDollarQuotedStrings) {
      let r = El(e, "");
      e.next == 36 && (e.advance(), kk(e, r), e.acceptToken(tn));
    } else if (i == 39 || i == 34 && n.doubleQuotedStrings)
      Fi(e, i, n.backslashEscapes), e.acceptToken(tn);
    else if (i == 35 && n.hashComments || i == 47 && e.next == 47 && n.slashComments)
      pf(e), e.acceptToken(hf);
    else if (i == 45 && e.next == 45 && (!n.spaceAfterDashes || e.peek(1) == 32))
      pf(e), e.acceptToken(hf);
    else if (i == 47 && e.next == 42) {
      e.advance();
      for (let r = 1; ; ) {
        let s = e.next;
        if (e.next < 0)
          break;
        if (e.advance(), s == 42 && e.next == 47) {
          if (r--, e.advance(), !r)
            break;
        } else s == 47 && e.next == 42 && (r++, e.advance());
      }
      e.acceptToken(nk);
    } else if ((i == 101 || i == 69) && e.next == 39)
      e.advance(), Fi(e, 39, !0), e.acceptToken(tn);
    else if ((i == 110 || i == 78) && e.next == 39 && n.charSetCasts)
      e.advance(), Fi(e, 39, n.backslashEscapes), e.acceptToken(tn);
    else if (i == 95 && n.charSetCasts)
      for (let r = 0; ; r++) {
        if (e.next == 39 && r > 1) {
          e.advance(), Fi(e, 39, n.backslashEscapes), e.acceptToken(tn);
          break;
        }
        if (!Bl(e.next))
          break;
        e.advance();
      }
    else if (n.plsqlQuotingMechanism && (i == 113 || i == 81) && e.next == 39 && e.peek(1) > 0 && !Bi(e.peek(1), Wo)) {
      let r = e.peek(1);
      e.advance(2), wk(e, r), e.acceptToken(tn);
    } else if (Bi(i, n.identifierQuotes)) {
      const r = i == 91 ? 93 : i;
      Fi(e, r, !1), e.acceptToken(vk);
    } else if (i == 40)
      e.acceptToken(ok);
    else if (i == 41)
      e.acceptToken(lk);
    else if (i == 123)
      e.acceptToken(ak);
    else if (i == 125)
      e.acceptToken(hk);
    else if (i == 91)
      e.acceptToken(ck);
    else if (i == 93)
      e.acceptToken(fk);
    else if (i == 59)
      e.acceptToken(uk);
    else if (n.unquotedBitLiterals && i == 48 && e.next == 98)
      e.advance(), uf(e), e.acceptToken(ff);
    else if ((i == 98 || i == 66) && (e.next == 39 || e.next == 34)) {
      const r = e.next;
      e.advance(), n.treatBitsAsBytes ? (Fi(e, r, n.backslashEscapes), e.acceptToken(yk)) : (uf(e, r), e.acceptToken(ff));
    } else if (i == 48 && (e.next == 120 || e.next == 88) || (i == 120 || i == 88) && e.next == 39) {
      let r = e.next == 39;
      for (e.advance(); xk(e.next); )
        e.advance();
      r && e.next == 39 && e.advance(), e.acceptToken(Fo);
    } else if (i == 46 && e.next >= 48 && e.next <= 57)
      df(e, !0), e.acceptToken(Fo);
    else if (i == 46)
      e.acceptToken(dk);
    else if (i >= 48 && i <= 57)
      df(e, !1), e.acceptToken(Fo);
    else if (Bi(i, n.operatorChars)) {
      for (; Bi(e.next, n.operatorChars); )
        e.advance();
      e.acceptToken(pk);
    } else if (Bi(i, n.specialVar))
      e.next == i && e.advance(), Sk(e), e.acceptToken(mk);
    else if (i == 58 || i == 44)
      e.acceptToken(gk);
    else if (Bl(i)) {
      let r = El(e, String.fromCharCode(i));
      e.acceptToken(e.next == 46 || e.peek(-r.length - 1) == 46 ? cf : (t = n.words[r.toLowerCase()]) !== null && t !== void 0 ? t : cf);
    }
  });
}
const wp = /* @__PURE__ */ kp(Il), Mk = /* @__PURE__ */ An.deserialize({
  version: 14,
  states: "%vQ]QQOOO#wQRO'#DSO$OQQO'#CwO%eQQO'#CxO%lQQO'#CyO%sQQO'#CzOOQQ'#DS'#DSOOQQ'#C}'#C}O'UQRO'#C{OOQQ'#Cv'#CvOOQQ'#C|'#C|Q]QQOOQOQQOOO'`QQO'#DOO(xQRO,59cO)PQQO,59cO)UQQO'#DSOOQQ,59d,59dO)cQQO,59dOOQQ,59e,59eO)jQQO,59eOOQQ,59f,59fO)qQQO,59fOOQQ-E6{-E6{OOQQ,59b,59bOOQQ-E6z-E6zOOQQ,59j,59jOOQQ-E6|-E6|O+VQRO1G.}O+^QQO,59cOOQQ1G/O1G/OOOQQ1G/P1G/POOQQ1G/Q1G/QP+kQQO'#C}O+rQQO1G.}O)PQQO,59cO,PQQO'#Cw",
  stateData: ",[~OtOSPOSQOS~ORUOSUOTUOUUOVROXSOZTO]XO^QO_UO`UOaPObPOcPOdUOeUOfUOgUOhUO~O^]ORvXSvXTvXUvXVvXXvXZvX]vX_vX`vXavXbvXcvXdvXevXfvXgvXhvX~OsvX~P!jOa_Ob_Oc_O~ORUOSUOTUOUUOVROXSOZTO^tO_UO`UOa`Ob`Oc`OdUOeUOfUOgUOhUO~OWaO~P$ZOYcO~P$ZO[eO~P$ZORUOSUOTUOUUOVROXSOZTO^QO_UO`UOaPObPOcPOdUOeUOfUOgUOhUO~O]hOsoX~P%zOajObjOcjO~O^]ORkaSkaTkaUkaVkaXkaZka]ka_ka`kaakabkackadkaekafkagkahka~Oska~P'kO^]O~OWvXYvX[vX~P!jOWnO~P$ZOYoO~P$ZO[pO~P$ZO^]ORkiSkiTkiUkiVkiXkiZki]ki_ki`kiakibkickidkiekifkigkihki~Oski~P)xOWkaYka[ka~P'kO]hO~P$ZOWkiYki[ki~P)xOasObsOcsO~O",
  goto: "#hwPPPPPPPPPPPPPPPPPPPPPPPPPPx||||!Y!^!d!xPPP#[TYOZeUORSTWZbdfqT[OZQZORiZSWOZQbRQdSQfTZgWbdfqQ^PWk^lmrQl_Qm`RrseVORSTWZbdfq",
  nodeNames: "⚠ LineComment BlockComment String Number Bool Null ( ) { } [ ] ; . Operator Punctuation SpecialVar Identifier QuotedIdentifier Keyword Type Bits Bytes Builtin Script Statement CompositeIdentifier Parens Braces Brackets Statement",
  maxTerm: 38,
  nodeProps: [
    ["isolate", -4, 1, 2, 3, 19, ""]
  ],
  skippedNodes: [0, 1, 2],
  repeatNodeCount: 3,
  tokenData: "RORO",
  tokenizers: [0, wp],
  topRules: { Script: [0, 25] },
  tokenPrec: 0
});
function Nl(n) {
  let e = n.cursor().moveTo(n.from, -1);
  for (; /Comment/.test(e.name); )
    e.moveTo(e.from, -1);
  return e.node;
}
function hr(n, e) {
  let t = n.sliceString(e.from, e.to), i = /^([`'"\[])(.*)([`'"\]])$/.exec(t);
  return i ? i[2] : t;
}
function Rs(n) {
  return n && (n.name == "Identifier" || n.name == "QuotedIdentifier");
}
function Tk(n, e) {
  if (e.name == "CompositeIdentifier") {
    let t = [];
    for (let i = e.firstChild; i; i = i.nextSibling)
      Rs(i) && t.push(hr(n, i));
    return t;
  }
  return [hr(n, e)];
}
function gf(n, e) {
  for (let t = []; ; ) {
    if (!e || e.name != ".")
      return t;
    let i = Nl(e);
    if (!Rs(i))
      return t;
    t.unshift(hr(n, i)), e = Nl(i);
  }
}
function Pk(n, e) {
  let t = Ve(n).resolveInner(e, -1), i = Lk(n.doc, t);
  return t.name == "Identifier" || t.name == "QuotedIdentifier" || t.name == "Keyword" ? {
    from: t.from,
    quoted: t.name == "QuotedIdentifier" ? n.doc.sliceString(t.from, t.from + 1) : null,
    parents: gf(n.doc, Nl(t)),
    aliases: i
  } : t.name == "." ? { from: e, quoted: null, parents: gf(n.doc, t), aliases: i } : { from: e, quoted: null, parents: [], empty: !0, aliases: i };
}
const Rk = /* @__PURE__ */ new Set(/* @__PURE__ */ "where group having order union intersect except all distinct limit offset fetch for".split(" "));
function Lk(n, e) {
  let t;
  for (let r = e; !t; r = r.parent) {
    if (!r)
      return null;
    r.name == "Statement" && (t = r);
  }
  let i = null;
  for (let r = t.firstChild, s = !1, o = null; r; r = r.nextSibling) {
    let l = r.name == "Keyword" ? n.sliceString(r.from, r.to).toLowerCase() : null, a = null;
    if (!s)
      s = l == "from";
    else if (l == "as" && o && Rs(r.nextSibling))
      a = hr(n, r.nextSibling);
    else {
      if (l && Rk.has(l))
        break;
      o && Rs(r) && (a = hr(n, r));
    }
    a && (i || (i = /* @__PURE__ */ Object.create(null)), i[a] = Tk(n, o)), o = /Identifier$/.test(r.name) ? r : null;
  }
  return i;
}
function Dk(n, e, t) {
  return t.map((i) => ({ ...i, label: i.label[0] == n ? i.label : n + i.label + e, apply: void 0 }));
}
const Bk = /^\w*$/, Ek = /^[`'"\[]?\w*[`'"\]]?$/;
function mf(n) {
  return n.self && typeof n.self.label == "string";
}
class Pa {
  constructor(e, t) {
    this.idQuote = e, this.idCaseInsensitive = t, this.list = [], this.children = void 0;
  }
  child(e) {
    let t = this.children || (this.children = /* @__PURE__ */ Object.create(null)), i = t[e];
    return i || (e && !this.list.some((r) => r.label == e) && this.list.push(vf(e, "type", this.idQuote, this.idCaseInsensitive)), t[e] = new Pa(this.idQuote, this.idCaseInsensitive));
  }
  maybeChild(e) {
    return this.children ? this.children[e] : null;
  }
  addCompletion(e) {
    let t = this.list.findIndex((i) => i.label == e.label);
    t > -1 ? this.list[t] = e : this.list.push(e);
  }
  addCompletions(e) {
    for (let t of e)
      this.addCompletion(typeof t == "string" ? vf(t, "property", this.idQuote, this.idCaseInsensitive) : t);
  }
  addNamespace(e) {
    Array.isArray(e) ? this.addCompletions(e) : mf(e) ? this.addNamespace(e.children) : this.addNamespaceObject(e);
  }
  addNamespaceObject(e) {
    for (let t of Object.keys(e)) {
      let i = e[t], r = null, s = t.replace(/\\?\./g, (l) => l == "." ? "\0" : l).split("\0"), o = this;
      mf(i) && (r = i.self, i = i.children);
      for (let l = 0; l < s.length; l++)
        r && l == s.length - 1 && o.addCompletion(r), o = o.child(s[l].replace(/\\\./g, "."));
      o.addNamespace(i);
    }
  }
}
function vf(n, e, t, i) {
  return new RegExp("^[a-z_][a-z_\\d]*$", i ? "i" : "").test(n) ? { label: n, type: e } : { label: n, type: e, apply: t + n + Sp(t) };
}
function Sp(n) {
  return n === "[" ? "]" : n;
}
function Ik(n, e, t, i, r, s) {
  var o;
  let l = ((o = s?.spec.identifierQuotes) === null || o === void 0 ? void 0 : o[0]) || '"', a = new Pa(l, !!s?.spec.caseInsensitiveIdentifiers), f = r ? a.child(r) : null;
  return a.addNamespace(n), e && (f || a).addCompletions(e), t && a.addCompletions(t), f && a.addCompletions(f.list), i && a.addCompletions((f || a).child(i).list), (u) => {
    let { parents: g, from: y, quoted: b, empty: k, aliases: C } = Pk(u.state, u.pos);
    if (k && !u.explicit)
      return null;
    C && g.length == 1 && (g = C[g[0]] || g);
    let M = a;
    for (let I of g) {
      for (; !M.children || !M.children[I]; )
        if (M == a && f)
          M = f;
        else if (M == f && i)
          M = M.child(i);
        else
          return null;
      let N = M.maybeChild(I);
      if (!N)
        return null;
      M = N;
    }
    let R = M.list;
    if (M == a && C && (R = R.concat(Object.keys(C).map((I) => ({ label: I, type: "constant" })))), b) {
      let I = b[0], N = Sp(I), z = u.state.sliceDoc(u.pos, u.pos + 1) == N;
      return {
        from: y,
        to: z ? u.pos + 1 : void 0,
        options: Dk(I, N, R),
        validFor: Ek
      };
    } else
      return {
        from: y,
        options: R,
        validFor: Bk
      };
  };
}
function Nk(n) {
  return n == bp ? "type" : n == yp ? "keyword" : "variable";
}
function Fk(n, e, t) {
  let i = Object.keys(n).map((r) => t(e ? r.toUpperCase() : r, Nk(n[r])));
  return Ix(["QuotedIdentifier", "String", "LineComment", "BlockComment", "."], Ca(i));
}
let Wk = /* @__PURE__ */ Mk.configure({
  props: [
    /* @__PURE__ */ Hs.add({
      Statement: /* @__PURE__ */ rr()
    }),
    /* @__PURE__ */ zs.add({
      Statement(n, e) {
        return { from: Math.min(n.from + 100, e.doc.lineAt(n.from).to), to: n.to };
      },
      BlockComment(n) {
        return { from: n.from + 2, to: n.to - 2 };
      }
    }),
    /* @__PURE__ */ Qs({
      Keyword: P.keyword,
      Type: P.typeName,
      Builtin: /* @__PURE__ */ P.standard(P.name),
      Bits: P.number,
      Bytes: P.string,
      Bool: P.bool,
      Null: P.null,
      Number: P.number,
      String: P.string,
      Identifier: P.name,
      QuotedIdentifier: /* @__PURE__ */ P.special(P.string),
      SpecialVar: /* @__PURE__ */ P.special(P.name),
      LineComment: P.lineComment,
      BlockComment: P.blockComment,
      Operator: P.operator,
      "Semi Punctuation": P.punctuation,
      "( )": P.paren,
      "{ }": P.brace,
      "[ ]": P.squareBracket
    })
  ]
});
class Ls {
  constructor(e, t, i) {
    this.dialect = e, this.language = t, this.spec = i;
  }
  /**
  Returns the language for this dialect as an extension.
  */
  get extension() {
    return this.language.extension;
  }
  /**
  Reconfigure the parser used by this dialect. Returns a new
  dialect object.
  */
  configureLanguage(e, t) {
    return new Ls(this.dialect, this.language.configure(e, t), this.spec);
  }
  /**
  Define a new dialect.
  */
  static define(e) {
    let t = Ak(e, e.keywords, e.types, e.builtin), i = Ui.define({
      name: "sql",
      parser: Wk.configure({
        tokenizers: [{ from: wp, to: kp(t) }]
      }),
      languageData: {
        commentTokens: { line: "--", block: { open: "/*", close: "*/" } },
        closeBrackets: { brackets: ["(", "[", "{", "'", '"', "`"] }
      }
    });
    return new Ls(t, i, e);
  }
}
function Qk(n, e) {
  return { label: n, type: e, boost: -1 };
}
function Vk(n, e = !1, t) {
  return Fk(n.dialect.words, e, t || Qk);
}
function Hk(n) {
  return n.schema ? Ik(n.schema, n.tables, n.schemas, n.defaultTable, n.defaultSchema, n.dialect || Ra) : () => null;
}
function zk(n) {
  return n.schema ? (n.dialect || Ra).language.data.of({
    autocomplete: Hk(n)
  }) : [];
}
function Ew(n = {}) {
  let e = n.dialect || Ra;
  return new ha(e.language, [
    zk(n),
    e.language.data.of({
      autocomplete: Vk(e, n.upperCaseKeywords, n.keywordCompletion)
    })
  ]);
}
const Ra = /* @__PURE__ */ Ls.define({});
function $k(n) {
  var e = n.Pos;
  function t(h, c, d) {
    if (c.line === d.line && c.ch >= d.ch - 1) {
      var p = h.getLine(c.line), m = p.charCodeAt(c.ch);
      55296 <= m && m <= 55551 && (d.ch += 1);
    }
    return { start: c, end: d };
  }
  var i = [
    // Key to key mapping. This goes first to make it possible to override
    // existing mappings.
    { keys: "<Left>", type: "keyToKey", toKeys: "h" },
    { keys: "<Right>", type: "keyToKey", toKeys: "l" },
    { keys: "<Up>", type: "keyToKey", toKeys: "k" },
    { keys: "<Down>", type: "keyToKey", toKeys: "j" },
    { keys: "g<Up>", type: "keyToKey", toKeys: "gk" },
    { keys: "g<Down>", type: "keyToKey", toKeys: "gj" },
    { keys: "<Space>", type: "keyToKey", toKeys: "l" },
    { keys: "<BS>", type: "keyToKey", toKeys: "h" },
    { keys: "<Del>", type: "keyToKey", toKeys: "x" },
    { keys: "<C-Space>", type: "keyToKey", toKeys: "W" },
    { keys: "<C-BS>", type: "keyToKey", toKeys: "B" },
    { keys: "<S-Space>", type: "keyToKey", toKeys: "w" },
    { keys: "<S-BS>", type: "keyToKey", toKeys: "b" },
    { keys: "<C-n>", type: "keyToKey", toKeys: "j" },
    { keys: "<C-p>", type: "keyToKey", toKeys: "k" },
    { keys: "<C-[>", type: "keyToKey", toKeys: "<Esc>" },
    { keys: "<C-c>", type: "keyToKey", toKeys: "<Esc>" },
    { keys: "<C-[>", type: "keyToKey", toKeys: "<Esc>", context: "insert" },
    { keys: "<C-c>", type: "keyToKey", toKeys: "<Esc>", context: "insert" },
    { keys: "<C-Esc>", type: "keyToKey", toKeys: "<Esc>" },
    // ipad keyboard sends C-Esc instead of C-[
    { keys: "<C-Esc>", type: "keyToKey", toKeys: "<Esc>", context: "insert" },
    { keys: "s", type: "keyToKey", toKeys: "cl", context: "normal" },
    { keys: "s", type: "keyToKey", toKeys: "c", context: "visual" },
    { keys: "S", type: "keyToKey", toKeys: "cc", context: "normal" },
    { keys: "S", type: "keyToKey", toKeys: "VdO", context: "visual" },
    { keys: "<Home>", type: "keyToKey", toKeys: "0" },
    { keys: "<End>", type: "keyToKey", toKeys: "$" },
    { keys: "<PageUp>", type: "keyToKey", toKeys: "<C-b>" },
    { keys: "<PageDown>", type: "keyToKey", toKeys: "<C-f>" },
    { keys: "<CR>", type: "keyToKey", toKeys: "j^", context: "normal" },
    { keys: "<Ins>", type: "keyToKey", toKeys: "i", context: "normal" },
    { keys: "<Ins>", type: "action", action: "toggleOverwrite", context: "insert" },
    // Motions
    { keys: "H", type: "motion", motion: "moveToTopLine", motionArgs: { linewise: !0, toJumplist: !0 } },
    { keys: "M", type: "motion", motion: "moveToMiddleLine", motionArgs: { linewise: !0, toJumplist: !0 } },
    { keys: "L", type: "motion", motion: "moveToBottomLine", motionArgs: { linewise: !0, toJumplist: !0 } },
    { keys: "h", type: "motion", motion: "moveByCharacters", motionArgs: { forward: !1 } },
    { keys: "l", type: "motion", motion: "moveByCharacters", motionArgs: { forward: !0 } },
    { keys: "j", type: "motion", motion: "moveByLines", motionArgs: { forward: !0, linewise: !0 } },
    { keys: "k", type: "motion", motion: "moveByLines", motionArgs: { forward: !1, linewise: !0 } },
    { keys: "gj", type: "motion", motion: "moveByDisplayLines", motionArgs: { forward: !0 } },
    { keys: "gk", type: "motion", motion: "moveByDisplayLines", motionArgs: { forward: !1 } },
    { keys: "w", type: "motion", motion: "moveByWords", motionArgs: { forward: !0, wordEnd: !1 } },
    { keys: "W", type: "motion", motion: "moveByWords", motionArgs: { forward: !0, wordEnd: !1, bigWord: !0 } },
    { keys: "e", type: "motion", motion: "moveByWords", motionArgs: { forward: !0, wordEnd: !0, inclusive: !0 } },
    { keys: "E", type: "motion", motion: "moveByWords", motionArgs: { forward: !0, wordEnd: !0, bigWord: !0, inclusive: !0 } },
    { keys: "b", type: "motion", motion: "moveByWords", motionArgs: { forward: !1, wordEnd: !1 } },
    { keys: "B", type: "motion", motion: "moveByWords", motionArgs: { forward: !1, wordEnd: !1, bigWord: !0 } },
    { keys: "ge", type: "motion", motion: "moveByWords", motionArgs: { forward: !1, wordEnd: !0, inclusive: !0 } },
    { keys: "gE", type: "motion", motion: "moveByWords", motionArgs: { forward: !1, wordEnd: !0, bigWord: !0, inclusive: !0 } },
    { keys: "{", type: "motion", motion: "moveByParagraph", motionArgs: { forward: !1, toJumplist: !0 } },
    { keys: "}", type: "motion", motion: "moveByParagraph", motionArgs: { forward: !0, toJumplist: !0 } },
    { keys: "(", type: "motion", motion: "moveBySentence", motionArgs: { forward: !1 } },
    { keys: ")", type: "motion", motion: "moveBySentence", motionArgs: { forward: !0 } },
    { keys: "<C-f>", type: "motion", motion: "moveByPage", motionArgs: { forward: !0 } },
    { keys: "<C-b>", type: "motion", motion: "moveByPage", motionArgs: { forward: !1 } },
    { keys: "<C-d>", type: "motion", motion: "moveByScroll", motionArgs: { forward: !0, explicitRepeat: !0 } },
    { keys: "<C-u>", type: "motion", motion: "moveByScroll", motionArgs: { forward: !1, explicitRepeat: !0 } },
    { keys: "gg", type: "motion", motion: "moveToLineOrEdgeOfDocument", motionArgs: { forward: !1, explicitRepeat: !0, linewise: !0, toJumplist: !0 } },
    { keys: "G", type: "motion", motion: "moveToLineOrEdgeOfDocument", motionArgs: { forward: !0, explicitRepeat: !0, linewise: !0, toJumplist: !0 } },
    { keys: "g$", type: "motion", motion: "moveToEndOfDisplayLine" },
    { keys: "g^", type: "motion", motion: "moveToStartOfDisplayLine" },
    { keys: "g0", type: "motion", motion: "moveToStartOfDisplayLine" },
    { keys: "0", type: "motion", motion: "moveToStartOfLine" },
    { keys: "^", type: "motion", motion: "moveToFirstNonWhiteSpaceCharacter" },
    { keys: "+", type: "motion", motion: "moveByLines", motionArgs: { forward: !0, toFirstChar: !0 } },
    { keys: "-", type: "motion", motion: "moveByLines", motionArgs: { forward: !1, toFirstChar: !0 } },
    { keys: "_", type: "motion", motion: "moveByLines", motionArgs: { forward: !0, toFirstChar: !0, repeatOffset: -1 } },
    { keys: "$", type: "motion", motion: "moveToEol", motionArgs: { inclusive: !0 } },
    { keys: "%", type: "motion", motion: "moveToMatchedSymbol", motionArgs: { inclusive: !0, toJumplist: !0 } },
    { keys: "f<character>", type: "motion", motion: "moveToCharacter", motionArgs: { forward: !0, inclusive: !0 } },
    { keys: "F<character>", type: "motion", motion: "moveToCharacter", motionArgs: { forward: !1 } },
    { keys: "t<character>", type: "motion", motion: "moveTillCharacter", motionArgs: { forward: !0, inclusive: !0 } },
    { keys: "T<character>", type: "motion", motion: "moveTillCharacter", motionArgs: { forward: !1 } },
    { keys: ";", type: "motion", motion: "repeatLastCharacterSearch", motionArgs: { forward: !0 } },
    { keys: ",", type: "motion", motion: "repeatLastCharacterSearch", motionArgs: { forward: !1 } },
    { keys: "'<register>", type: "motion", motion: "goToMark", motionArgs: { toJumplist: !0, linewise: !0 } },
    { keys: "`<register>", type: "motion", motion: "goToMark", motionArgs: { toJumplist: !0 } },
    { keys: "]`", type: "motion", motion: "jumpToMark", motionArgs: { forward: !0 } },
    { keys: "[`", type: "motion", motion: "jumpToMark", motionArgs: { forward: !1 } },
    { keys: "]'", type: "motion", motion: "jumpToMark", motionArgs: { forward: !0, linewise: !0 } },
    { keys: "['", type: "motion", motion: "jumpToMark", motionArgs: { forward: !1, linewise: !0 } },
    // the next two aren't motions but must come before more general motion declarations
    { keys: "]p", type: "action", action: "paste", isEdit: !0, actionArgs: { after: !0, isEdit: !0, matchIndent: !0 } },
    { keys: "[p", type: "action", action: "paste", isEdit: !0, actionArgs: { after: !1, isEdit: !0, matchIndent: !0 } },
    { keys: "]<character>", type: "motion", motion: "moveToSymbol", motionArgs: { forward: !0, toJumplist: !0 } },
    { keys: "[<character>", type: "motion", motion: "moveToSymbol", motionArgs: { forward: !1, toJumplist: !0 } },
    { keys: "|", type: "motion", motion: "moveToColumn" },
    { keys: "o", type: "motion", motion: "moveToOtherHighlightedEnd", context: "visual" },
    { keys: "O", type: "motion", motion: "moveToOtherHighlightedEnd", motionArgs: { sameLine: !0 }, context: "visual" },
    // Operators
    { keys: "d", type: "operator", operator: "delete" },
    { keys: "y", type: "operator", operator: "yank" },
    { keys: "c", type: "operator", operator: "change" },
    { keys: "=", type: "operator", operator: "indentAuto" },
    { keys: ">", type: "operator", operator: "indent", operatorArgs: { indentRight: !0 } },
    { keys: "<", type: "operator", operator: "indent", operatorArgs: { indentRight: !1 } },
    { keys: "g~", type: "operator", operator: "changeCase" },
    { keys: "gu", type: "operator", operator: "changeCase", operatorArgs: { toLower: !0 }, isEdit: !0 },
    { keys: "gU", type: "operator", operator: "changeCase", operatorArgs: { toLower: !1 }, isEdit: !0 },
    { keys: "n", type: "motion", motion: "findNext", motionArgs: { forward: !0, toJumplist: !0 } },
    { keys: "N", type: "motion", motion: "findNext", motionArgs: { forward: !1, toJumplist: !0 } },
    { keys: "gn", type: "motion", motion: "findAndSelectNextInclusive", motionArgs: { forward: !0 } },
    { keys: "gN", type: "motion", motion: "findAndSelectNextInclusive", motionArgs: { forward: !1 } },
    { keys: "gq", type: "operator", operator: "hardWrap" },
    { keys: "gw", type: "operator", operator: "hardWrap", operatorArgs: { keepCursor: !0 } },
    { keys: "g?", type: "operator", operator: "rot13" },
    // Operator-Motion dual commands
    { keys: "x", type: "operatorMotion", operator: "delete", motion: "moveByCharacters", motionArgs: { forward: !0 }, operatorMotionArgs: { visualLine: !1 } },
    { keys: "X", type: "operatorMotion", operator: "delete", motion: "moveByCharacters", motionArgs: { forward: !1 }, operatorMotionArgs: { visualLine: !0 } },
    { keys: "D", type: "operatorMotion", operator: "delete", motion: "moveToEol", motionArgs: { inclusive: !0 }, context: "normal" },
    { keys: "D", type: "operator", operator: "delete", operatorArgs: { linewise: !0 }, context: "visual" },
    { keys: "Y", type: "operatorMotion", operator: "yank", motion: "expandToLine", motionArgs: { linewise: !0 }, context: "normal" },
    { keys: "Y", type: "operator", operator: "yank", operatorArgs: { linewise: !0 }, context: "visual" },
    { keys: "C", type: "operatorMotion", operator: "change", motion: "moveToEol", motionArgs: { inclusive: !0 }, context: "normal" },
    { keys: "C", type: "operator", operator: "change", operatorArgs: { linewise: !0 }, context: "visual" },
    { keys: "~", type: "operatorMotion", operator: "changeCase", motion: "moveByCharacters", motionArgs: { forward: !0 }, operatorArgs: { shouldMoveCursor: !0 }, context: "normal" },
    { keys: "~", type: "operator", operator: "changeCase", context: "visual" },
    { keys: "<C-u>", type: "operatorMotion", operator: "delete", motion: "moveToStartOfLine", context: "insert" },
    { keys: "<C-w>", type: "operatorMotion", operator: "delete", motion: "moveByWords", motionArgs: { forward: !1, wordEnd: !1 }, context: "insert" },
    //ignore C-w in normal mode
    { keys: "<C-w>", type: "idle", context: "normal" },
    // Actions
    { keys: "<C-i>", type: "action", action: "jumpListWalk", actionArgs: { forward: !0 } },
    { keys: "<C-o>", type: "action", action: "jumpListWalk", actionArgs: { forward: !1 } },
    { keys: "<C-e>", type: "action", action: "scroll", actionArgs: { forward: !0, linewise: !0 } },
    { keys: "<C-y>", type: "action", action: "scroll", actionArgs: { forward: !1, linewise: !0 } },
    { keys: "a", type: "action", action: "enterInsertMode", isEdit: !0, actionArgs: { insertAt: "charAfter" }, context: "normal" },
    { keys: "A", type: "action", action: "enterInsertMode", isEdit: !0, actionArgs: { insertAt: "eol" }, context: "normal" },
    { keys: "A", type: "action", action: "enterInsertMode", isEdit: !0, actionArgs: { insertAt: "endOfSelectedArea" }, context: "visual" },
    { keys: "i", type: "action", action: "enterInsertMode", isEdit: !0, actionArgs: { insertAt: "inplace" }, context: "normal" },
    { keys: "gi", type: "action", action: "enterInsertMode", isEdit: !0, actionArgs: { insertAt: "lastEdit" }, context: "normal" },
    { keys: "I", type: "action", action: "enterInsertMode", isEdit: !0, actionArgs: { insertAt: "firstNonBlank" }, context: "normal" },
    { keys: "gI", type: "action", action: "enterInsertMode", isEdit: !0, actionArgs: { insertAt: "bol" }, context: "normal" },
    { keys: "I", type: "action", action: "enterInsertMode", isEdit: !0, actionArgs: { insertAt: "startOfSelectedArea" }, context: "visual" },
    { keys: "o", type: "action", action: "newLineAndEnterInsertMode", isEdit: !0, interlaceInsertRepeat: !0, actionArgs: { after: !0 }, context: "normal" },
    { keys: "O", type: "action", action: "newLineAndEnterInsertMode", isEdit: !0, interlaceInsertRepeat: !0, actionArgs: { after: !1 }, context: "normal" },
    { keys: "v", type: "action", action: "toggleVisualMode" },
    { keys: "V", type: "action", action: "toggleVisualMode", actionArgs: { linewise: !0 } },
    { keys: "<C-v>", type: "action", action: "toggleVisualMode", actionArgs: { blockwise: !0 } },
    { keys: "<C-q>", type: "action", action: "toggleVisualMode", actionArgs: { blockwise: !0 } },
    { keys: "gv", type: "action", action: "reselectLastSelection" },
    { keys: "J", type: "action", action: "joinLines", isEdit: !0 },
    { keys: "gJ", type: "action", action: "joinLines", actionArgs: { keepSpaces: !0 }, isEdit: !0 },
    { keys: "p", type: "action", action: "paste", isEdit: !0, actionArgs: { after: !0, isEdit: !0 } },
    { keys: "P", type: "action", action: "paste", isEdit: !0, actionArgs: { after: !1, isEdit: !0 } },
    { keys: "r<character>", type: "action", action: "replace", isEdit: !0 },
    { keys: "@<register>", type: "action", action: "replayMacro" },
    { keys: "q<register>", type: "action", action: "enterMacroRecordMode" },
    // Handle Replace-mode as a special case of insert mode.
    { keys: "R", type: "action", action: "enterInsertMode", isEdit: !0, actionArgs: { replace: !0 }, context: "normal" },
    { keys: "R", type: "operator", operator: "change", operatorArgs: { linewise: !0, fullLine: !0 }, context: "visual", exitVisualBlock: !0 },
    { keys: "u", type: "action", action: "undo", context: "normal" },
    { keys: "u", type: "operator", operator: "changeCase", operatorArgs: { toLower: !0 }, context: "visual", isEdit: !0 },
    { keys: "U", type: "operator", operator: "changeCase", operatorArgs: { toLower: !1 }, context: "visual", isEdit: !0 },
    { keys: "<C-r>", type: "action", action: "redo" },
    { keys: "m<register>", type: "action", action: "setMark" },
    { keys: '"<register>', type: "action", action: "setRegister" },
    { keys: "<C-r><register>", type: "action", action: "insertRegister", context: "insert", isEdit: !0 },
    { keys: "<C-o>", type: "action", action: "oneNormalCommand", context: "insert" },
    { keys: "zz", type: "action", action: "scrollToCursor", actionArgs: { position: "center" } },
    { keys: "z.", type: "action", action: "scrollToCursor", actionArgs: { position: "center" }, motion: "moveToFirstNonWhiteSpaceCharacter" },
    { keys: "zt", type: "action", action: "scrollToCursor", actionArgs: { position: "top" } },
    { keys: "z<CR>", type: "action", action: "scrollToCursor", actionArgs: { position: "top" }, motion: "moveToFirstNonWhiteSpaceCharacter" },
    { keys: "zb", type: "action", action: "scrollToCursor", actionArgs: { position: "bottom" } },
    { keys: "z-", type: "action", action: "scrollToCursor", actionArgs: { position: "bottom" }, motion: "moveToFirstNonWhiteSpaceCharacter" },
    { keys: ".", type: "action", action: "repeatLastEdit" },
    { keys: "<C-a>", type: "action", action: "incrementNumberToken", isEdit: !0, actionArgs: { increase: !0, backtrack: !1 } },
    { keys: "<C-x>", type: "action", action: "incrementNumberToken", isEdit: !0, actionArgs: { increase: !1, backtrack: !1 } },
    { keys: "<C-t>", type: "action", action: "indent", actionArgs: { indentRight: !0 }, context: "insert" },
    { keys: "<C-d>", type: "action", action: "indent", actionArgs: { indentRight: !1 }, context: "insert" },
    // Text object motions
    { keys: "a<register>", type: "motion", motion: "textObjectManipulation" },
    { keys: "i<register>", type: "motion", motion: "textObjectManipulation", motionArgs: { textObjectInner: !0 } },
    // Search
    { keys: "/", type: "search", searchArgs: { forward: !0, querySrc: "prompt", toJumplist: !0 } },
    { keys: "?", type: "search", searchArgs: { forward: !1, querySrc: "prompt", toJumplist: !0 } },
    { keys: "*", type: "search", searchArgs: { forward: !0, querySrc: "wordUnderCursor", wholeWordOnly: !0, toJumplist: !0 } },
    { keys: "#", type: "search", searchArgs: { forward: !1, querySrc: "wordUnderCursor", wholeWordOnly: !0, toJumplist: !0 } },
    { keys: "g*", type: "search", searchArgs: { forward: !0, querySrc: "wordUnderCursor", toJumplist: !0 } },
    { keys: "g#", type: "search", searchArgs: { forward: !1, querySrc: "wordUnderCursor", toJumplist: !0 } },
    // Ex command
    { keys: ":", type: "ex" }
  ], r = /* @__PURE__ */ Object.create(null), s = i.length, o = [
    { name: "colorscheme", shortName: "colo" },
    { name: "map" },
    { name: "imap", shortName: "im" },
    { name: "nmap", shortName: "nm" },
    { name: "vmap", shortName: "vm" },
    { name: "omap", shortName: "om" },
    { name: "noremap", shortName: "no" },
    { name: "nnoremap", shortName: "nn" },
    { name: "vnoremap", shortName: "vn" },
    { name: "inoremap", shortName: "ino" },
    { name: "onoremap", shortName: "ono" },
    { name: "unmap" },
    { name: "mapclear", shortName: "mapc" },
    { name: "nmapclear", shortName: "nmapc" },
    { name: "vmapclear", shortName: "vmapc" },
    { name: "imapclear", shortName: "imapc" },
    { name: "omapclear", shortName: "omapc" },
    { name: "write", shortName: "w" },
    { name: "undo", shortName: "u" },
    { name: "redo", shortName: "red" },
    { name: "set", shortName: "se" },
    { name: "setlocal", shortName: "setl" },
    { name: "setglobal", shortName: "setg" },
    { name: "sort", shortName: "sor" },
    { name: "substitute", shortName: "s", possiblyAsync: !0 },
    { name: "startinsert", shortName: "start" },
    { name: "nohlsearch", shortName: "noh" },
    { name: "yank", shortName: "y" },
    { name: "delmarks", shortName: "delm" },
    { name: "marks", excludeFromCommandHistory: !0 },
    { name: "registers", shortName: "reg", excludeFromCommandHistory: !0 },
    { name: "vglobal", shortName: "v" },
    { name: "delete", shortName: "d" },
    { name: "join", shortName: "j" },
    { name: "normal", shortName: "norm" },
    { name: "global", shortName: "g" }
  ], l = Da("");
  function a(h) {
    h.setOption("disableInput", !0), h.setOption("showCursorWhenSelecting", !1), n.signal(h, "vim-mode-change", { mode: "normal" }), h.on("cursorActivity", rh), Be(h), n.on(h.getInputField(), "paste", u(h));
  }
  function f(h) {
    h.setOption("disableInput", !1), h.off("cursorActivity", rh), n.off(h.getInputField(), "paste", u(h)), h.state.vim = null, Dn && clearTimeout(Dn);
  }
  function u(h) {
    var c = h.state.vim;
    return c.onPasteFn || (c.onPasteFn = function() {
      c.insertMode || (h.setCursor(Ie(h.getCursor(), 0, 1)), Zi.enterInsertMode(h, {}, c));
    }), c.onPasteFn;
  }
  var g = /[\d]/, y = [n.isWordChar, function(h) {
    return h && !n.isWordChar(h) && !/\s/.test(h);
  }], b = [function(h) {
    return /\S/.test(h);
  }], k = ["<", ">"], C = ["-", '"', ".", ":", "_", "/", "+"], M = /^\w$/, R = /^[A-Z]$/;
  try {
    R = new RegExp("^[\\p{Lu}]$", "u");
  } catch {
  }
  function I(h, c) {
    return c >= h.firstLine() && c <= h.lastLine();
  }
  function N(h) {
    return /^[a-z]$/.test(h);
  }
  function z(h) {
    return "()[]{}".indexOf(h) != -1;
  }
  function F(h) {
    return g.test(h);
  }
  function H(h) {
    return R.test(h);
  }
  function Q(h) {
    return /^\s*$/.test(h);
  }
  function Z(h) {
    return ".?!".indexOf(h) != -1;
  }
  function le(h, c) {
    for (var d = 0; d < c.length; d++)
      if (c[d] == h)
        return !0;
    return !1;
  }
  var he = {};
  function ee(h, c, d, p, m) {
    if (c === void 0 && !m)
      throw Error("defaultValue is required unless callback is provided");
    if (d || (d = "string"), he[h] = {
      type: d,
      defaultValue: c,
      callback: m
    }, p)
      for (var v = 0; v < p.length; v++)
        he[p[v]] = he[h];
    c && Y(h, c);
  }
  function Y(h, c, d, p) {
    var m = he[h];
    p = p || {};
    var v = p.scope;
    if (!m)
      return new Error("Unknown option: " + h);
    if (m.type == "boolean") {
      if (c && c !== !0)
        return new Error("Invalid argument: " + h + "=" + c);
      c !== !1 && (c = !0);
    }
    m.callback ? (v !== "local" && m.callback(c, void 0), v !== "global" && d && m.callback(c, d)) : (v !== "local" && (m.value = m.type == "boolean" ? !!c : c), v !== "global" && d && (d.state.vim.options[h] = { value: c }));
  }
  function ie(h, c, d) {
    var p = he[h];
    d = d || {};
    var m = d.scope;
    if (!p)
      return new Error("Unknown option: " + h);
    if (p.callback) {
      let v = c && p.callback(void 0, c);
      return m !== "global" && v !== void 0 ? v : m !== "local" ? p.callback() : void 0;
    } else
      return (m !== "global" && c && c.state.vim.options[h] || m !== "local" && p || {}).value;
  }
  ee("filetype", void 0, "string", ["ft"], function(h, c) {
    if (c !== void 0)
      if (h === void 0) {
        let d = c.getOption("mode");
        return d == "null" ? "" : d;
      } else {
        let d = h == "" ? "null" : h;
        c.setOption("mode", d);
      }
  }), ee("textwidth", 80, "number", ["tw"], function(h, c) {
    if (c !== void 0)
      if (h === void 0) {
        var d = c.getOption("textwidth");
        return d;
      } else {
        var p = Math.round(h);
        p > 1 && c.setOption("textwidth", p);
      }
  });
  var fe = function() {
    var h = 100, c = -1, d = 0, p = 0, m = (
      /**@type {(Marker|undefined)[]} */
      new Array(h)
    );
    function v(S, A, O) {
      var L = c % h, D = m[L];
      function B(W) {
        var V = ++c % h, X = m[V];
        X && X.clear(), m[V] = S.setBookmark(W);
      }
      if (D) {
        var T = D.find();
        T && !xt(T, A) && B(A);
      } else
        B(A);
      B(O), d = c, p = c - h + 1, p < 0 && (p = 0);
    }
    function x(S, A) {
      c += A, c > d ? c = d : c < p && (c = p);
      var O = m[(h + c) % h];
      if (O && !O.find()) {
        var L = A > 0 ? 1 : -1, D, B = S.getCursor();
        do
          if (c += L, O = m[(h + c) % h], O && (D = O.find()) && !xt(B, D))
            break;
        while (c < d && c > p);
      }
      return O;
    }
    function w(S, A) {
      var O = c, L = x(S, A);
      return c = O, L && L.find();
    }
    return {
      /**@type{Pos|undefined} */
      cachedCursor: void 0,
      //used for # and * jumps
      add: v,
      find: w,
      move: x
    };
  }, me = function(h) {
    return h ? {
      changes: h.changes,
      expectCursorActivityForChange: h.expectCursorActivityForChange
    } : {
      // Change list
      changes: [],
      // Set to true on change, false on cursorActivity.
      expectCursorActivityForChange: !1
    };
  };
  class qe {
    constructor() {
      this.latestRegister = void 0, this.isPlaying = !1, this.isRecording = !1, this.replaySearchQueries = [], this.onRecordingDone = void 0, this.lastInsertModeChanges = me();
    }
    exitMacroRecordMode() {
      var c = q.macroModeState;
      c.onRecordingDone && c.onRecordingDone(), c.onRecordingDone = void 0, c.isRecording = !1;
    }
    /**
     * @arg {CodeMirror} cm
     * @arg {string} registerName
     */
    enterMacroRecordMode(c, d) {
      var p = q.registerController.getRegister(d);
      if (p) {
        if (p.clear(), this.latestRegister = d, c.openDialog) {
          var m = Et("span", { class: "cm-vim-message" }, "recording @" + d);
          this.onRecordingDone = c.openDialog(m, function() {
          }, { bottom: !0 });
        }
        this.isRecording = !0;
      }
    }
  }
  function Be(h) {
    return h.state.vim || (h.state.vim = {
      inputState: new Ba(),
      // Vim's input state that triggered the last edit, used to repeat
      // motions and operators with '.'.
      lastEditInputState: void 0,
      // Vim's action command before the last edit, used to repeat actions
      // with '.' and insert mode repeat.
      lastEditActionCommand: void 0,
      // When using jk for navigation, if you move from a longer line to a
      // shorter line, the cursor may clip to the end of the shorter line.
      // If j is pressed again and cursor goes to the next line, the
      // cursor should go back to its horizontal position on the longer
      // line if it can. This is to keep track of the horizontal position.
      lastHPos: -1,
      // Doing the same with screen-position for gj/gk
      lastHSPos: -1,
      // The last motion command run. Cleared if a non-motion command gets
      // executed in between.
      lastMotion: null,
      marks: {},
      insertMode: !1,
      insertModeReturn: !1,
      // Repeat count for changes made in insert mode, triggered by key
      // sequences like 3,i. Only exists when insertMode is true.
      insertModeRepeat: void 0,
      visualMode: !1,
      // If we are in visual line mode. No effect if visualMode is false.
      visualLine: !1,
      visualBlock: !1,
      lastSelection: (
        /**@type{vimState["lastSelection"]}*/
        /**@type{unknown}*/
        null
      ),
      lastPastedText: void 0,
      sel: { anchor: new e(0, 0), head: new e(0, 0) },
      // Buffer-local/window-local values of vim options.
      options: {},
      // Whether the next character should be interpreted literally
      // Necassary for correct implementation of f<character>, r<character> etc.
      // in terms of langmaps.
      expectLiteralNext: !1,
      status: ""
    }), h.state.vim;
  }
  var q;
  function Ee() {
    q = {
      // The current search query.
      searchQuery: null,
      // Whether we are searching backwards.
      searchIsReversed: !1,
      // Replace part of the last substituted pattern
      lastSubstituteReplacePart: void 0,
      jumpList: fe(),
      macroModeState: new qe(),
      // Recording latest f, t, F or T motion command.
      lastCharacterSearch: { increment: 0, forward: !0, selectedCharacter: "" },
      registerController: new Wp({}),
      // search history buffer
      searchHistoryController: new Ea(),
      // ex Command history buffer
      exCommandHistoryController: new Ea()
    };
    for (var h in he) {
      var c = he[h];
      c.value = c.defaultValue;
    }
  }
  class Ge {
    /**
     * Wrapper for special keys pressed in insert mode
     * @arg {string} keyName
     * @arg {KeyboardEvent} e
     * @returns
     */
    constructor(c, d) {
      this.keyName = c, this.key = d.key, this.ctrlKey = d.ctrlKey, this.altKey = d.altKey, this.metaKey = d.metaKey, this.shiftKey = d.shiftKey;
    }
  }
  var tt, we = {
    enterVimMode: a,
    leaveVimMode: f,
    buildKeyMap: function() {
    },
    // Testing hook, though it might be useful to expose the register
    // controller anyway.
    getRegisterController: function() {
      return q.registerController;
    },
    // Testing hook.
    resetVimGlobalState_: Ee,
    // Testing hook.
    getVimGlobalState_: function() {
      return q;
    },
    // Testing hook.
    maybeInitVimState_: Be,
    suppressErrorLogging: !1,
    InsertModeKey: Ge,
    /**@type {(lhs: string, rhs: string, ctx: string) => void} */
    map: function(h, c, d) {
      lt.map(h, c, d);
    },
    /**@type {(lhs: string, ctx: string) => any} */
    unmap: function(h, c) {
      return lt.unmap(h, c);
    },
    // Non-recursive map function.
    // NOTE: This will not create mappings to key maps that aren't present
    // in the default key map. See TODO at bottom of function.
    /**@type {(lhs: string, rhs: string, ctx: string) => void} */
    noremap: function(h, c, d) {
      lt.map(h, c, d, !0);
    },
    // Remove all user-defined mappings for the provided context.
    /**@arg {string} [ctx]} */
    mapclear: function(h) {
      var c = i.length, d = s, p = i.slice(0, c - d);
      if (i = i.slice(c - d), h)
        for (var m = p.length - 1; m >= 0; m--) {
          var v = p[m];
          if (h !== v.context)
            if (v.context)
              this._mapCommand(v);
            else {
              var x = ["normal", "insert", "visual"];
              for (var w in x)
                if (x[w] !== h) {
                  var S = Object.assign({}, v);
                  S.context = x[w], this._mapCommand(S);
                }
            }
        }
    },
    langmap: La,
    vimKeyFromEvent: Rn,
    // TODO: Expose setOption and getOption as instance methods. Need to decide how to namespace
    // them, or somehow make them work with the existing CodeMirror setOption/getOption API.
    setOption: Y,
    getOption: ie,
    defineOption: ee,
    /**@type {(name: string, prefix: string|undefined, func: ExFn) => void} */
    defineEx: function(h, c, d) {
      if (!c)
        c = h;
      else if (h.indexOf(c) !== 0)
        throw new Error('(Vim.defineEx) "' + c + '" is not a prefix of "' + h + '", command not registered');
      ih[h] = d, lt.commandMap_[c] = { name: h, shortName: c, type: "api" };
    },
    /**@type {(cm: CodeMirror, key: string, origin: string) => undefined | boolean} */
    handleKey: function(h, c, d) {
      var p = this.findKey(h, c, d);
      if (typeof p == "function")
        return p();
    },
    multiSelectHandleKey: Mg,
    /**
     * This is the outermost function called by CodeMirror, after keys have
     * been mapped to their Vim equivalents.
     *
     * Finds a command based on the key (and cached keys if there is a
     * multi-key sequence). Returns `undefined` if no key is matched, a noop
     * function if a partial match is found (multi-key), and a function to
     * execute the bound command if a a key is matched. The function always
     * returns true.
     */
    /**@type {(cm_: CodeMirror, key: string, origin?: string| undefined) => (() => boolean|undefined) | undefined} */
    findKey: function(h, c, d) {
      var p = Be(h), m = (
        /**@type {CodeMirrorV}*/
        h
      );
      function v() {
        var O = q.macroModeState;
        if (O.isRecording) {
          if (c == "q")
            return O.exitMacroRecordMode(), He(m), !0;
          d != "mapping" && Cg(O, c);
        }
      }
      function x() {
        if (c == "<Esc>") {
          if (p.visualMode)
            zt(m);
          else if (p.insertMode)
            di(m);
          else
            return;
          return He(m), !0;
        }
      }
      function w() {
        if (x())
          return !0;
        p.inputState.keyBuffer.push(c);
        var O = p.inputState.keyBuffer.join(""), L = c.length == 1, D = Ri.matchCommand(O, i, p.inputState, "insert"), B = p.inputState.changeQueue;
        if (D.type == "none")
          return He(m), !1;
        if (D.type == "partial") {
          if (D.expectLiteralNext && (p.expectLiteralNext = !0), tt && window.clearTimeout(tt), tt = L && window.setTimeout(
            function() {
              p.insertMode && p.inputState.keyBuffer.length && He(m);
            },
            ie("insertModeEscKeysTimeout")
          ), L) {
            var T = m.listSelections();
            (!B || B.removed.length != T.length) && (B = p.inputState.changeQueue = new Np()), B.inserted += c;
            for (var W = 0; W < T.length; W++) {
              var V = nt(T[W].anchor, T[W].head), X = ui(T[W].anchor, T[W].head), K = m.getRange(V, m.state.overwrite ? Ie(X, 0, 1) : X);
              B.removed[W] = (B.removed[W] || "") + K;
            }
          }
          return !L;
        } else D.type == "full" && (p.inputState.keyBuffer.length = 0);
        if (p.expectLiteralNext = !1, tt && window.clearTimeout(tt), D.command && B) {
          for (var T = m.listSelections(), W = 0; W < T.length; W++) {
            var te = T[W].head;
            m.replaceRange(
              B.removed[W] || "",
              Ie(te, 0, -B.inserted.length),
              te,
              "+input"
            );
          }
          q.macroModeState.lastInsertModeChanges.changes.pop();
        }
        return D.command || He(m), D.command;
      }
      function S() {
        if (v() || x())
          return !0;
        p.inputState.keyBuffer.push(c);
        var O = p.inputState.keyBuffer.join("");
        if (/^[1-9]\d*$/.test(O))
          return !0;
        var L = /^(\d*)(.*)$/.exec(O);
        if (!L)
          return He(m), !1;
        var D = p.visualMode ? "visual" : "normal", B = L[2] || L[1];
        p.inputState.operatorShortcut && p.inputState.operatorShortcut.slice(-1) == B && (B = p.inputState.operatorShortcut);
        var T = Ri.matchCommand(B, i, p.inputState, D);
        return T.type == "none" ? (He(m), !1) : T.type == "partial" ? (T.expectLiteralNext && (p.expectLiteralNext = !0), !0) : T.type == "clear" ? (He(m), !0) : (p.expectLiteralNext = !1, p.inputState.keyBuffer.length = 0, L = /^(\d*)(.*)$/.exec(O), L && L[1] && L[1] != "0" && p.inputState.pushRepeatDigit(L[1]), T.command);
      }
      var A = p.insertMode ? w() : S();
      if (A === !1)
        return !p.insertMode && (c.length === 1 || n.isMac && /<A-.>/.test(c)) ? function() {
          return !0;
        } : void 0;
      if (A === !0)
        return function() {
          return !0;
        };
      if (A)
        return function() {
          return m.operation(function() {
            m.curOp.isVimOp = !0;
            try {
              if (typeof A != "object") return;
              A.type == "keyToKey" ? fi(m, A.toKeys, A) : Ri.processCommand(m, p, A);
            } catch (O) {
              throw m.state.vim = void 0, Be(m), we.suppressErrorLogging || console.log(O), O;
            }
            return !0;
          });
        };
    },
    /**@type {(cm: CodeMirrorV, input: string)=>void} */
    handleEx: function(h, c) {
      lt.processCommand(h, c);
    },
    defineMotion: Qp,
    defineAction: Hp,
    defineOperator: Vp,
    mapCommand: wg,
    _mapCommand: no,
    defineRegister: Fp,
    exitVisualMode: zt,
    exitInsertMode: di
  }, Ke = [], bt = !1, ve;
  function Yi(h) {
    if (!ve) throw new Error("No prompt to send key to");
    if (h[0] == "<") {
      var c = h.toLowerCase().slice(1, -1), d = c.split("-");
      if (c = d.pop() || "", c == "lt") h = "<";
      else if (c == "space") h = " ";
      else if (c == "cr") h = `
`;
      else if (Gi[c]) {
        var p = ve.value || "", m = {
          key: Gi[c],
          target: {
            value: p,
            selectionEnd: p.length,
            selectionStart: p.length
          }
        };
        ve.onKeyDown && ve.onKeyDown(m, ve.value, x), ve && ve.onKeyUp && ve.onKeyUp(m, ve.value, x);
        return;
      }
    }
    if (h == `
`) {
      var v = ve;
      ve = null, v.onClose && v.onClose(v.value);
    } else
      ve.value = (ve.value || "") + h;
    function x(w) {
      ve && (typeof w == "string" ? ve.value = w : ve = null);
    }
  }
  function fi(h, c, d) {
    var p = bt;
    if (d) {
      if (Ke.indexOf(d) != -1) return;
      Ke.push(d), bt = d.noremap != !1;
    }
    try {
      for (var m = Be(h), v = /<(?:[CSMA]-)*\w+>|./gi, x; x = v.exec(c); ) {
        var w = x[0], S = m.insertMode;
        if (ve) {
          Yi(w);
          continue;
        }
        var A = we.handleKey(h, w, "mapping");
        if (!A && S && m.insertMode) {
          if (w[0] == "<") {
            var O = w.toLowerCase().slice(1, -1), L = O.split("-");
            if (O = L.pop() || "", O == "lt") w = "<";
            else if (O == "space") w = " ";
            else if (O == "cr") w = `
`;
            else if (Gi.hasOwnProperty(O)) {
              w = Gi[O], ah(h, w);
              continue;
            } else
              w = w[0], v.lastIndex = x.index + 1;
          }
          h.replaceSelection(w);
        }
      }
    } finally {
      if (Ke.pop(), bt = Ke.length ? p : !1, !Ke.length && ve) {
        var D = ve;
        ve = null, br(h, D);
      }
    }
  }
  var Xs = {
    Return: "CR",
    Backspace: "BS",
    Delete: "Del",
    Escape: "Esc",
    Insert: "Ins",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    ArrowUp: "Up",
    ArrowDown: "Down",
    Enter: "CR",
    " ": "Space"
  }, Ip = {
    Shift: 1,
    Alt: 1,
    Command: 1,
    Control: 1,
    CapsLock: 1,
    AltGraph: 1,
    Dead: 1,
    Unidentified: 1
  }, Gi = {};
  "Left|Right|Up|Down|End|Home".split("|").concat(Object.keys(Xs)).forEach(function(h) {
    Gi[(Xs[h] || "").toLowerCase()] = Gi[h.toLowerCase()] = h;
  });
  function Rn(h, c) {
    var d = h.key;
    if (!Ip[d]) {
      d.length > 1 && d[0] == "n" && (d = d.replace("Numpad", "")), d = Xs[d] || d;
      var p = "";
      if (h.ctrlKey && (p += "C-"), h.altKey && (p += "A-"), h.metaKey && (p += "M-"), n.isMac && p == "A-" && d.length == 1 && (p = p.slice(2)), (p || d.length > 1) && h.shiftKey && (p += "S-"), c && !c.expectLiteralNext && d.length == 1) {
        if (l.keymap && d in l.keymap)
          (l.remapCtrl != !1 || !p) && (d = l.keymap[d]);
        else if (d.charCodeAt(0) > 128 && !r[d]) {
          var m = h.code?.slice(-1) || "";
          h.shiftKey || (m = m.toLowerCase()), m && (d = m, !p && h.altKey && (p = "A-"));
        }
      }
      return p += d, p.length > 1 && (p = "<" + p + ">"), p;
    }
  }
  function La(h, c) {
    l.string !== h && (l = Da(h)), l.remapCtrl = c;
  }
  function Da(h) {
    let c = {};
    if (!h) return { keymap: c, string: "" };
    function d(p) {
      return p.split(/\\?(.)/).filter(Boolean);
    }
    return h.split(/((?:[^\\,]|\\.)+),/).map((p) => {
      if (!p) return;
      const m = p.split(/((?:[^\\;]|\\.)+);/);
      if (m.length == 3) {
        const v = d(m[1]), x = d(m[2]);
        if (v.length !== x.length) return;
        for (let w = 0; w < v.length; ++w) c[v[w]] = x[w];
      } else if (m.length == 1) {
        const v = d(p);
        if (v.length % 2 !== 0) return;
        for (let x = 0; x < v.length; x += 2) c[v[x]] = v[x + 1];
      }
    }), { keymap: c, string: h };
  }
  ee("langmap", void 0, "string", ["lmap"], function(h, c) {
    if (h === void 0)
      return l.string;
    La(h);
  });
  class Ba {
    constructor() {
      this.prefixRepeat = [], this.motionRepeat = [], this.operator = null, this.operatorArgs = null, this.motion = null, this.motionArgs = null, this.keyBuffer = [], this.registerName = void 0, this.changeQueue = null;
    }
    /** @param {string} n */
    pushRepeatDigit(c) {
      this.operator ? this.motionRepeat = this.motionRepeat.concat(c) : this.prefixRepeat = this.prefixRepeat.concat(c);
    }
    getRepeat() {
      var c = 0;
      return (this.prefixRepeat.length > 0 || this.motionRepeat.length > 0) && (c = 1, this.prefixRepeat.length > 0 && (c *= parseInt(this.prefixRepeat.join(""), 10)), this.motionRepeat.length > 0 && (c *= parseInt(this.motionRepeat.join(""), 10))), c;
    }
  }
  function He(h, c) {
    h.state.vim.inputState = new Ba(), h.state.vim.expectLiteralNext = !1, n.signal(h, "vim-command-done", c);
  }
  function Np() {
    this.removed = [], this.inserted = "";
  }
  class Vt {
    /** @arg {string} [text] @arg {boolean} [linewise] @arg {boolean } [blockwise] */
    constructor(c, d, p) {
      this.clear(), this.keyBuffer = [c || ""], this.insertModeChanges = [], this.searchQueries = [], this.linewise = !!d, this.blockwise = !!p;
    }
    /** @arg {string} [text] @arg {boolean} [linewise] @arg {boolean } [blockwise] */
    setText(c, d, p) {
      this.keyBuffer = [c || ""], this.linewise = !!d, this.blockwise = !!p;
    }
    /** @arg {string} text @arg {boolean} [linewise] */
    pushText(c, d) {
      d && (this.linewise || this.keyBuffer.push(`
`), this.linewise = !0), this.keyBuffer.push(c);
    }
    /** @arg {InsertModeChanges} changes */
    pushInsertModeChanges(c) {
      this.insertModeChanges.push(me(c));
    }
    /** @arg {string} query */
    pushSearchQuery(c) {
      this.searchQueries.push(c);
    }
    clear() {
      this.keyBuffer = [], this.insertModeChanges = [], this.searchQueries = [], this.linewise = !1;
    }
    toString() {
      return this.keyBuffer.join("");
    }
  }
  function Fp(h, c) {
    var d = q.registerController.registers;
    if (!h || h.length != 1)
      throw Error("Register name must be 1 character");
    if (d[h])
      throw Error("Register already defined " + h);
    d[h] = c, C.push(h);
  }
  class Wp {
    /** @arg {Object<string, Register>} registers */
    constructor(c) {
      this.registers = c, this.unnamedRegister = c['"'] = new Vt(), c["."] = new Vt(), c[":"] = new Vt(), c["/"] = new Vt(), c["+"] = new Vt();
    }
    /**
     * @param {string | null | undefined} registerName
     * @param {string} operator
     * @param {string} text
     * @param {boolean} [linewise]
     * @param {boolean} [blockwise]
     */
    pushText(c, d, p, m, v) {
      if (c !== "_") {
        m && p.charAt(p.length - 1) !== `
` && (p += `
`);
        var x = this.isValidRegister(c) ? this.getRegister(c) : null;
        if (!x || !c) {
          switch (d) {
            case "yank":
              this.registers[0] = new Vt(p, m, v);
              break;
            case "delete":
            case "change":
              p.indexOf(`
`) == -1 ? this.registers["-"] = new Vt(p, m) : (this.shiftNumericRegisters_(), this.registers[1] = new Vt(p, m));
              break;
          }
          this.unnamedRegister.setText(p, m, v);
          return;
        }
        var w = H(c);
        w ? x.pushText(p, m) : x.setText(p, m, v), c === "+" && navigator.clipboard.writeText(p), this.unnamedRegister.setText(x.toString(), m);
      }
    }
    /**
     * Gets the register named @name.  If one of @name doesn't already exist,
     * create it.  If @name is invalid, return the unnamedRegister.
     * @arg {string} [name]
     */
    getRegister(c) {
      return this.isValidRegister(c) ? (c = c.toLowerCase(), this.registers[c] || (this.registers[c] = new Vt()), this.registers[c]) : this.unnamedRegister;
    }
    /**@type {{(name: any): name is string}} */
    isValidRegister(c) {
      return c && (le(c, C) || M.test(c));
    }
    shiftNumericRegisters_() {
      for (var c = 9; c >= 2; c--)
        this.registers[c] = this.getRegister("" + (c - 1));
    }
  }
  class Ea {
    constructor() {
      this.historyBuffer = [], this.iterator = 0, this.initialPrefix = null;
    }
    /**
     * the input argument here acts a user entered prefix for a small time
     * until we start autocompletion in which case it is the autocompleted.
     * @arg {string} input
     * @arg {boolean} up
     */
    nextMatch(c, d) {
      var p = this.historyBuffer, m = d ? -1 : 1;
      this.initialPrefix === null && (this.initialPrefix = c);
      for (var v = this.iterator + m; d ? v >= 0 : v < p.length; v += m)
        for (var x = p[v], w = 0; w <= x.length; w++)
          if (this.initialPrefix == x.substring(0, w))
            return this.iterator = v, x;
      if (v >= p.length)
        return this.iterator = p.length, this.initialPrefix;
      if (v < 0) return c;
    }
    /** @arg {string} input */
    pushInput(c) {
      var d = this.historyBuffer.indexOf(c);
      d > -1 && this.historyBuffer.splice(d, 1), c.length && this.historyBuffer.push(c);
    }
    reset() {
      this.initialPrefix = null, this.iterator = this.historyBuffer.length;
    }
  }
  var Ri = {
    /**
     * @param {string} keys
     * @param {vimKey[]} keyMap
     * @param {InputStateInterface} inputState
     * @param {string} context
     */
    matchCommand: function(h, c, d, p) {
      var m = zp(h, c, p, d), v = m.full[0];
      if (!v)
        return m.partial.length ? {
          type: "partial",
          expectLiteralNext: m.partial.length == 1 && m.partial[0].keys.slice(-11) == "<character>"
          // langmap literal logic
        } : { type: "none" };
      if (v.keys.slice(-11) == "<character>" || v.keys.slice(-10) == "<register>") {
        var x = qp(h);
        if (!x || x.length > 1) return { type: "clear" };
        d.selectedCharacter = x;
      }
      return { type: "full", command: v };
    },
    /**
     * @arg {CodeMirrorV} cm
     * @arg {vimState} vim
     * @arg {vimKey} command
     */
    processCommand: function(h, c, d) {
      switch (c.inputState.repeatOverride = d.repeatOverride, d.type) {
        case "motion":
          this.processMotion(h, c, d);
          break;
        case "operator":
          this.processOperator(h, c, d);
          break;
        case "operatorMotion":
          this.processOperatorMotion(h, c, d);
          break;
        case "action":
          this.processAction(h, c, d);
          break;
        case "search":
          this.processSearch(h, c, d);
          break;
        case "ex":
        case "keyToEx":
          this.processEx(h, c, d);
          break;
      }
    },
    /**
     * @arg {CodeMirrorV} cm
     * @arg {vimState} vim
     * @arg {import("./types").motionCommand|import("./types").operatorMotionCommand} command
     */
    processMotion: function(h, c, d) {
      c.inputState.motion = d.motion, c.inputState.motionArgs = /**@type {MotionArgs}*/
      yr(d.motionArgs), this.evalInput(h, c);
    },
    /**
     * @arg {CodeMirrorV} cm
     * @arg {vimState} vim
     * @arg {import("./types").operatorCommand|import("./types").operatorMotionCommand} command
     */
    processOperator: function(h, c, d) {
      var p = c.inputState;
      if (p.operator)
        if (p.operator == d.operator) {
          p.motion = "expandToLine", p.motionArgs = { linewise: !0, repeat: 1 }, this.evalInput(h, c);
          return;
        } else
          He(h);
      p.operator = d.operator, p.operatorArgs = yr(d.operatorArgs), d.keys.length > 1 && (p.operatorShortcut = d.keys), d.exitVisualBlock && (c.visualBlock = !1, Ji(h)), c.visualMode && this.evalInput(h, c);
    },
    /**
     * @arg {CodeMirrorV} cm
     * @arg {vimState} vim
     * @arg {import("./types").operatorMotionCommand} command
     */
    processOperatorMotion: function(h, c, d) {
      var p = c.visualMode, m = yr(d.operatorMotionArgs);
      m && p && m.visualLine && (c.visualLine = !0), this.processOperator(h, c, d), p || this.processMotion(h, c, d);
    },
    /**
     * @arg {CodeMirrorV} cm
     * @arg {vimState} vim
     * @arg {import("./types").actionCommand} command
     */
    processAction: function(h, c, d) {
      var p = c.inputState, m = p.getRepeat(), v = !!m, x = (
        /**@type {ActionArgs}*/
        yr(d.actionArgs) || { repeat: 1 }
      );
      p.selectedCharacter && (x.selectedCharacter = p.selectedCharacter), d.operator && this.processOperator(h, c, d), d.motion && this.processMotion(h, c, d), (d.motion || d.operator) && this.evalInput(h, c), x.repeat = m || 1, x.repeatIsExplicit = v, x.registerName = p.registerName, He(h), c.lastMotion = null, d.isEdit && this.recordLastEdit(c, p, d), Zi[d.action](h, x, c);
    },
    /** @arg {CodeMirrorV} cm @arg {vimState} vim @arg {import("./types").searchCommand} command*/
    processSearch: function(h, c, d) {
      if (!h.getSearchCursor)
        return;
      var p = d.searchArgs.forward, m = d.searchArgs.wholeWordOnly;
      Bt(h).setReversed(!p);
      var v = p ? "/" : "?", x = Bt(h).getQuery(), w = h.getScrollInfo(), S = "";
      function A(K, te, ae) {
        q.searchHistoryController.pushInput(K), q.searchHistoryController.reset();
        try {
          Ln(h, K, te, ae);
        } catch {
          de(h, "Invalid regex: " + K), He(h);
          return;
        }
        Ri.processMotion(h, c, {
          keys: "",
          type: "motion",
          motion: "findNext",
          motionArgs: { forward: !0, toJumplist: d.searchArgs.toJumplist }
        });
      }
      function O(K) {
        h.scrollTo(w.left, w.top), A(
          K,
          !0,
          !0
          /** smartCase */
        );
        var te = q.macroModeState;
        te.isRecording && Ag(te, K);
      }
      function L() {
        return ie("pcre") ? "(JavaScript regexp: set pcre)" : "(Vim regexp: set nopcre)";
      }
      function D(K, te, ae) {
        var re = Rn(K), Ae, Pe;
        re == "<Up>" || re == "<Down>" ? (Ae = re == "<Up>", Pe = K.target ? K.target.selectionEnd : 0, te = q.searchHistoryController.nextMatch(te, Ae) || "", ae(te), Pe && K.target && (K.target.selectionEnd = K.target.selectionStart = Math.min(Pe, K.target.value.length))) : re && re != "<Left>" && re != "<Right>" && q.searchHistoryController.reset(), S = te, B();
      }
      function B() {
        var K;
        try {
          K = Ln(
            h,
            S,
            !0,
            !0
            /** smartCase */
          );
        } catch {
        }
        K ? h.scrollIntoView(eh(h, !p, K), 30) : (en(h), h.scrollTo(w.left, w.top));
      }
      function T(K, te, ae) {
        var re = Rn(K);
        re == "<Esc>" || re == "<C-c>" || re == "<C-[>" || re == "<BS>" && te == "" ? (q.searchHistoryController.pushInput(te), q.searchHistoryController.reset(), Ln(h, x?.source || ""), en(h), h.scrollTo(w.left, w.top), n.e_stop(K), He(h), ae(), h.focus()) : re == "<Up>" || re == "<Down>" ? n.e_stop(K) : re == "<C-u>" && (n.e_stop(K), ae(""));
      }
      switch (d.searchArgs.querySrc) {
        case "prompt":
          var W = q.macroModeState;
          if (W.isPlaying) {
            let te = W.replaySearchQueries.shift();
            A(
              te || "",
              !0,
              !1
              /** smartCase */
            );
          } else
            br(h, {
              onClose: O,
              prefix: v,
              desc: Et(
                "span",
                {
                  $cursor: "pointer",
                  onmousedown: function(te) {
                    te.preventDefault(), Y("pcre", !ie("pcre")), this.textContent = L(), B();
                  }
                },
                L()
              ),
              onKeyUp: D,
              onKeyDown: T
            });
          break;
        case "wordUnderCursor":
          var V = Js(h, { noSymbol: !0 }), X = !0;
          if (V || (V = Js(h, { noSymbol: !1 }), X = !1), !V) {
            de(h, "No word under cursor"), He(h);
            return;
          }
          let K = h.getLine(V.start.line).substring(
            V.start.ch,
            V.end.ch
          );
          X && m ? K = "\\b" + K + "\\b" : K = Kp(K), q.jumpList.cachedCursor = h.getCursor(), h.setCursor(V.start), A(
            K,
            !0,
            !1
            /** smartCase */
          );
          break;
      }
    },
    /**
     * @arg {CodeMirrorV} cm
     * @arg {vimState} vim
     * @arg {import("./types").exCommand | import("./types").keyToExCommand} command
     */
    processEx: function(h, c, d) {
      function p(w) {
        q.exCommandHistoryController.pushInput(w), q.exCommandHistoryController.reset(), lt.processCommand(h, w), h.state.vim && He(h), en(h);
      }
      function m(w, S, A) {
        var O = Rn(w), L, D;
        (O == "<Esc>" || O == "<C-c>" || O == "<C-[>" || O == "<BS>" && S == "") && (q.exCommandHistoryController.pushInput(S), q.exCommandHistoryController.reset(), n.e_stop(w), He(h), en(h), A(), h.focus()), O == "<Up>" || O == "<Down>" ? (n.e_stop(w), L = O == "<Up>", D = w.target ? w.target.selectionEnd : 0, S = q.exCommandHistoryController.nextMatch(S, L) || "", A(S), D && w.target && (w.target.selectionEnd = w.target.selectionStart = Math.min(D, w.target.value.length))) : O == "<C-u>" ? (n.e_stop(w), A("")) : O && O != "<Left>" && O != "<Right>" && q.exCommandHistoryController.reset();
      }
      function v(w, S) {
        var A = new n.StringStream(S), O = (
          /**@type{import("./types").exCommandArgs}*/
          {}
        );
        try {
          if (lt.parseInput_(h, A, O), O.commandName != "s") {
            en(h);
            return;
          }
          var L = lt.matchCommand_(O.commandName);
          if (!L || (lt.parseCommandArgs_(A, O, L), !O.argString)) return;
          var D = Ja(O.argString.slice(1), !0, !0);
          D && to(h, D);
        } catch {
        }
      }
      if (d.type == "keyToEx")
        lt.processCommand(h, d.exArgs.input);
      else {
        var x = {
          onClose: p,
          onKeyDown: m,
          onKeyUp: v,
          prefix: ":"
        };
        c.visualMode && (x.value = "'<,'>", x.selectValueOnOpen = !1), br(h, x);
      }
    },
    /**@arg {CodeMirrorV} cm   @arg {vimState} vim */
    evalInput: function(h, c) {
      var d = c.inputState, p = d.motion, m = d.motionArgs || { repeat: 1 }, v = d.operator, x = d.operatorArgs || {}, w = d.registerName, S = c.sel, A = Se(c.visualMode ? it(h, S.head) : h.getCursor("head")), O = Se(c.visualMode ? it(h, S.anchor) : h.getCursor("anchor")), L = Se(A), D = Se(O), B, T, W;
      if (v && this.recordLastEdit(c, d), d.repeatOverride !== void 0 ? W = d.repeatOverride : W = d.getRepeat(), W > 0 && m.explicitRepeat ? m.repeatIsExplicit = !0 : (m.noRepeat || !m.explicitRepeat && W === 0) && (W = 1, m.repeatIsExplicit = !1), d.selectedCharacter && (m.selectedCharacter = x.selectedCharacter = d.selectedCharacter), m.repeat = W, He(h), p) {
        var V = Ht[p](h, A, m, c, d);
        if (c.lastMotion = Ht[p], !V)
          return;
        if (m.toJumplist) {
          var X = q.jumpList, K = X.cachedCursor;
          K ? (Ha(h, K, V), delete X.cachedCursor) : Ha(h, A, V);
        }
        V instanceof Array ? (T = V[0], B = V[1]) : B = V, B || (B = Se(A)), c.visualMode ? (c.visualBlock && B.ch === 1 / 0 || (B = it(h, B, L)), T && (T = it(h, T)), T = T || D, S.anchor = T, S.head = B, Ji(h), ii(
          h,
          c,
          "<",
          Re(T, B) ? T : B
        ), ii(
          h,
          c,
          ">",
          Re(T, B) ? B : T
        )) : v || (B = it(h, B, L), h.setCursor(B.line, B.ch));
      }
      if (v) {
        if (x.lastSel) {
          T = D;
          var te = x.lastSel, ae = Math.abs(te.head.line - te.anchor.line), re = Math.abs(te.head.ch - te.anchor.ch);
          te.visualLine ? B = new e(D.line + ae, D.ch) : te.visualBlock ? B = new e(D.line + ae, D.ch + re) : te.head.line == te.anchor.line ? B = new e(D.line, D.ch + re) : B = new e(D.line + ae, D.ch), c.visualMode = !0, c.visualLine = te.visualLine, c.visualBlock = te.visualBlock, S = c.sel = {
            anchor: T,
            head: B
          }, Ji(h);
        } else c.visualMode && (x.lastSel = {
          anchor: Se(S.anchor),
          head: Se(S.head),
          visualBlock: c.visualBlock,
          visualLine: c.visualLine
        });
        var Ae, Pe, ue, J, Ce;
        if (c.visualMode) {
          Ae = nt(S.head, S.anchor), Pe = ui(S.head, S.anchor), ue = c.visualLine || x.linewise, J = c.visualBlock ? "block" : ue ? "line" : "char";
          var kt = t(h, Ae, Pe);
          if (Ce = Zs(h, {
            anchor: kt.start,
            head: kt.end
          }, J), ue) {
            var _e = Ce.ranges;
            if (J == "block")
              for (var wt = 0; wt < _e.length; wt++)
                _e[wt].head.ch = Ze(h, _e[wt].head.line);
            else J == "line" && (_e[0].head = new e(_e[0].head.line + 1, 0));
          }
        } else {
          if (Ae = Se(T || D), Pe = Se(B || L), Re(Pe, Ae)) {
            var Li = Ae;
            Ae = Pe, Pe = Li;
          }
          ue = m.linewise || x.linewise, ue ? Zp(h, Ae, Pe) : m.forward && Gp(h, Ae, Pe), J = "char";
          var Tg = !m.inclusive || ue, kt = t(h, Ae, Pe);
          Ce = Zs(h, {
            anchor: kt.start,
            head: kt.end
          }, J, Tg);
        }
        h.setSelections(Ce.ranges, Ce.primary), c.lastMotion = null, x.repeat = W, x.registerName = w, x.linewise = ue;
        var so = Ys[v](
          h,
          x,
          Ce.ranges,
          D,
          B
        );
        c.visualMode && zt(h, so != null), so && h.setCursor(so);
      }
    },
    /**@arg {vimState} vim  @arg {InputStateInterface} inputState, @arg {import("./types").actionCommand} [actionCommand] */
    recordLastEdit: function(h, c, d) {
      var p = q.macroModeState;
      p.isPlaying || (h.lastEditInputState = c, h.lastEditActionCommand = d, p.lastInsertModeChanges.changes = [], p.lastInsertModeChanges.expectCursorActivityForChange = !1, p.lastInsertModeChanges.visualBlock = h.visualBlock ? h.sel.head.line - h.sel.anchor.line : 0);
    }
  }, Ht = {
    moveToTopLine: function(h, c, d) {
      var p = io(h).top + d.repeat - 1;
      return new e(p, $t(h.getLine(p)));
    },
    moveToMiddleLine: function(h) {
      var c = io(h), d = Math.floor((c.top + c.bottom) * 0.5);
      return new e(d, $t(h.getLine(d)));
    },
    moveToBottomLine: function(h, c, d) {
      var p = io(h).bottom - d.repeat + 1;
      return new e(p, $t(h.getLine(p)));
    },
    expandToLine: function(h, c, d) {
      var p = c;
      return new e(p.line + d.repeat - 1, 1 / 0);
    },
    findNext: function(h, c, d) {
      var p = Bt(h), m = p.getQuery();
      if (m) {
        var v = !d.forward;
        v = p.isReversed() ? !v : v, to(h, m);
        var x = eh(h, v, m, d.repeat);
        return x || de(h, "No match found " + m + (ie("pcre") ? " (set nopcre to use Vim regexps)" : "")), x;
      }
    },
    /**
     * Find and select the next occurrence of the search query. If the cursor is currently
     * within a match, then find and select the current match. Otherwise, find the next occurrence in the
     * appropriate direction.
     *
     * This differs from `findNext` in the following ways:
     *
     * 1. Instead of only returning the "from", this returns a "from", "to" range.
     * 2. If the cursor is currently inside a search match, this selects the current match
     *    instead of the next match.
     * 3. If there is no associated operator, this will turn on visual mode.
     */
    findAndSelectNextInclusive: function(h, c, d, p, m) {
      var v = Bt(h), x = v.getQuery();
      if (x) {
        var w = !d.forward;
        w = v.isReversed() ? !w : w;
        var S = mg(h, w, x, d.repeat, p);
        if (S) {
          if (m.operator)
            return S;
          var A = S[0], O = new e(S[1].line, S[1].ch - 1);
          if (p.visualMode) {
            (p.visualLine || p.visualBlock) && (p.visualLine = !1, p.visualBlock = !1, n.signal(h, "vim-mode-change", { mode: "visual", subMode: "" }));
            var L = p.sel.anchor;
            if (L)
              return v.isReversed() ? d.forward ? [L, A] : [L, O] : d.forward ? [L, O] : [L, A];
          } else
            p.visualMode = !0, p.visualLine = !1, p.visualBlock = !1, n.signal(h, "vim-mode-change", { mode: "visual", subMode: "" });
          return w ? [O, A] : [A, O];
        }
      }
    },
    goToMark: function(h, c, d, p) {
      var m = xr(h, p, d.selectedCharacter || "");
      return m ? d.linewise ? { line: m.line, ch: $t(h.getLine(m.line)) } : m : null;
    },
    moveToOtherHighlightedEnd: function(h, c, d, p) {
      var m = p.sel;
      return p.visualBlock && d.sameLine ? [
        it(h, new e(m.anchor.line, m.head.ch)),
        it(h, new e(m.head.line, m.anchor.ch))
      ] : [m.head, m.anchor];
    },
    jumpToMark: function(h, c, d, p) {
      for (var m = c, v = 0; v < d.repeat; v++) {
        var x = m;
        for (var w in p.marks)
          if (N(w)) {
            var S = p.marks[w].find(), A = d.forward ? (
              // @ts-ignore
              Re(S, x)
            ) : Re(x, S);
            if (!A && !(d.linewise && S.line == x.line)) {
              var O = xt(x, m), L = d.forward ? (
                // @ts-ignore
                Fa(x, S, m)
              ) : (
                // @ts-ignore
                Fa(m, S, x)
              );
              (O || L) && (m = S);
            }
          }
      }
      return d.linewise && (m = new e(m.line, $t(h.getLine(m.line)))), m;
    },
    moveByCharacters: function(h, c, d) {
      var p = c, m = d.repeat, v = d.forward ? p.ch + m : p.ch - m;
      return new e(p.line, v);
    },
    moveByLines: function(h, c, d, p) {
      var m = c, v = m.ch;
      switch (p.lastMotion) {
        case this.moveByLines:
        case this.moveByDisplayLines:
        case this.moveByScroll:
        case this.moveToColumn:
        case this.moveToEol:
          v = p.lastHPos;
          break;
        default:
          p.lastHPos = v;
      }
      var x = d.repeat + (d.repeatOffset || 0), w = d.forward ? m.line + x : m.line - x, S = h.firstLine(), A = h.lastLine(), O = h.findPosV(m, d.forward ? x : -x, "line", p.lastHSPos), L = d.forward ? O.line > w : O.line < w;
      return L && (w = O.line, v = O.ch), w < S && m.line == S ? this.moveToStartOfLine(h, c, d, p) : w > A && m.line == A ? Ka(h, c, d, p, !0) : (d.toFirstChar && (v = $t(h.getLine(w)), p.lastHPos = v), p.lastHSPos = h.charCoords(new e(w, v), "div").left, new e(w, v));
    },
    moveByDisplayLines: function(h, c, d, p) {
      var m = c;
      switch (p.lastMotion) {
        case this.moveByDisplayLines:
        case this.moveByScroll:
        case this.moveByLines:
        case this.moveToColumn:
        case this.moveToEol:
          break;
        default:
          p.lastHSPos = h.charCoords(m, "div").left;
      }
      var v = d.repeat, x = h.findPosV(m, d.forward ? v : -v, "line", p.lastHSPos);
      if (x.hitSide)
        if (d.forward) {
          var w = h.charCoords(x, "div"), S = { top: w.top + 8, left: p.lastHSPos };
          x = h.coordsChar(S, "div");
        } else {
          var A = h.charCoords(new e(h.firstLine(), 0), "div");
          A.left = p.lastHSPos, x = h.coordsChar(A, "div");
        }
      return p.lastHPos = x.ch, x;
    },
    moveByPage: function(h, c, d) {
      var p = c, m = d.repeat;
      return h.findPosV(p, d.forward ? m : -m, "page");
    },
    moveByParagraph: function(h, c, d) {
      var p = d.forward ? 1 : -1;
      return _a(h, c, d.repeat, p).start;
    },
    moveBySentence: function(h, c, d) {
      var p = d.forward ? 1 : -1;
      return sg(h, c, d.repeat, p);
    },
    moveByScroll: function(h, c, d, p) {
      var m = h.getScrollInfo(), v = null, x = d.repeat;
      x || (x = m.clientHeight / (2 * h.defaultTextHeight()));
      var w = h.charCoords(c, "local");
      if (d.repeat = x, v = Ht.moveByDisplayLines(h, c, d, p), !v)
        return null;
      var S = h.charCoords(v, "local");
      return h.scrollTo(null, m.top + S.top - w.top), v;
    },
    moveByWords: function(h, c, d) {
      return ig(
        h,
        c,
        d.repeat,
        !!d.forward,
        !!d.wordEnd,
        !!d.bigWord
      );
    },
    moveTillCharacter: function(h, c, d) {
      var p = d.repeat, m = eo(
        h,
        p,
        d.forward,
        d.selectedCharacter,
        c
      ), v = d.forward ? -1 : 1;
      return za(v, d), m ? (m.ch += v, m) : null;
    },
    moveToCharacter: function(h, c, d) {
      var p = d.repeat;
      return za(0, d), eo(
        h,
        p,
        d.forward,
        d.selectedCharacter,
        c
      ) || c;
    },
    moveToSymbol: function(h, c, d) {
      var p = d.repeat;
      return d.selectedCharacter && tg(
        h,
        p,
        d.forward,
        d.selectedCharacter
      ) || c;
    },
    moveToColumn: function(h, c, d, p) {
      var m = d.repeat;
      return p.lastHPos = m - 1, p.lastHSPos = h.charCoords(c, "div").left, ng(h, m);
    },
    moveToEol: function(h, c, d, p) {
      return Ka(h, c, d, p, !1);
    },
    moveToFirstNonWhiteSpaceCharacter: function(h, c) {
      var d = c;
      return new e(
        d.line,
        $t(h.getLine(d.line))
      );
    },
    moveToMatchedSymbol: function(h, c) {
      for (var d = c, p = d.line, m = d.ch, v = h.getLine(p), x; m < v.length; m++)
        if (x = v.charAt(m), x && z(x)) {
          var w = h.getTokenTypeAt(new e(p, m + 1));
          if (w !== "string" && w !== "comment")
            break;
        }
      if (m < v.length) {
        var S = x === "<" || x === ">" ? /[(){}[\]<>]/ : /[(){}[\]]/, A = h.findMatchingBracket(new e(p, m), { bracketRegex: S });
        return A.to;
      } else
        return d;
    },
    moveToStartOfLine: function(h, c) {
      return new e(c.line, 0);
    },
    moveToLineOrEdgeOfDocument: function(h, c, d) {
      var p = d.forward ? h.lastLine() : h.firstLine();
      return d.repeatIsExplicit && (p = d.repeat - h.getOption("firstLineNumber")), new e(
        p,
        $t(h.getLine(p))
      );
    },
    moveToStartOfDisplayLine: function(h) {
      return h.execCommand("goLineLeft"), h.getCursor();
    },
    moveToEndOfDisplayLine: function(h) {
      h.execCommand("goLineRight");
      var c = h.getCursor();
      return c.sticky == "before" && c.ch--, c;
    },
    textObjectManipulation: function(h, c, d, p) {
      var m = {
        "(": ")",
        ")": "(",
        "{": "}",
        "}": "{",
        "[": "]",
        "]": "[",
        "<": ">",
        ">": "<"
      }, v = { "'": !0, '"': !0, "`": !0 }, x = d.selectedCharacter || "";
      x == "b" ? x = "(" : x == "B" && (x = "{");
      var w = !d.textObjectInner, S, A;
      if (m[x]) {
        if (A = !0, S = Ua(h, c, x, w), !S) {
          var O = h.getSearchCursor(new RegExp("\\" + x, "g"), c);
          O.find() && (S = Ua(h, O.from(), x, w));
        }
      } else if (v[x])
        A = !0, S = og(h, c, x, w);
      else if (x === "W" || x === "w")
        for (var L = d.repeat || 1; L-- > 0; ) {
          var D = Js(h, {
            inclusive: w,
            innerWord: !w,
            bigWord: x === "W",
            noSymbol: x === "W",
            multiline: !0
          }, S && S.end);
          D && (S || (S = D), S.end = D.end);
        }
      else if (x === "p")
        if (S = _a(h, c, d.repeat, 0, w), d.linewise = !0, p.visualMode)
          p.visualLine || (p.visualLine = !0);
        else {
          var B = p.inputState.operatorArgs;
          B && (B.linewise = !0), S.end.line--;
        }
      else if (x === "t")
        S = Jp(h, c, w);
      else if (x === "s") {
        var T = h.getLine(c.line);
        c.ch > 0 && Z(T[c.ch]) && (c.ch -= 1);
        var W = ja(h, c, d.repeat, 1, w), V = ja(h, c, d.repeat, -1, w);
        Q(h.getLine(V.line)[V.ch]) && Q(h.getLine(W.line)[W.ch - 1]) && (V = { line: V.line, ch: V.ch + 1 }), S = { start: V, end: W };
      }
      return S ? h.state.vim.visualMode ? Xp(h, S.start, S.end, A) : [S.start, S.end] : null;
    },
    repeatLastCharacterSearch: function(h, c, d) {
      var p = q.lastCharacterSearch, m = d.repeat, v = d.forward === p.forward, x = (p.increment ? 1 : 0) * (v ? -1 : 1);
      h.moveH(-x, "char"), d.inclusive = !!v;
      var w = eo(h, m, v, p.selectedCharacter);
      return w ? (w.ch += x, w) : (h.moveH(x, "char"), c);
    }
  };
  function Qp(h, c) {
    Ht[h] = c;
  }
  function Ia(h, c) {
    for (var d = [], p = 0; p < c; p++)
      d.push(h);
    return d;
  }
  var Ys = {
    change: function(h, c, d) {
      var p, m, v = h.state.vim, x = d[0].anchor, w = d[0].head;
      if (v.visualMode)
        if (c.fullLine)
          w.ch = Number.MAX_VALUE, w.line--, h.setSelection(x, w), m = h.getSelection(), h.replaceSelection(""), p = x;
        else {
          m = h.getSelection();
          var O = Ia("", d.length);
          h.replaceSelections(O), p = nt(d[0].head, d[0].anchor);
        }
      else {
        m = h.getRange(x, w);
        var S = v.lastEditInputState;
        if (S?.motion == "moveByWords" && !Q(m)) {
          var A = /\s+$/.exec(m);
          A && S.motionArgs && S.motionArgs.forward && (w = Ie(w, 0, -A[0].length), m = m.slice(0, -A[0].length));
        }
        c.linewise && (x = new e(x.line, $t(h.getLine(x.line))), w.line > x.line && (w = new e(w.line - 1, Number.MAX_VALUE))), h.replaceRange("", x, w), p = x;
      }
      q.registerController.pushText(
        c.registerName,
        "change",
        m,
        c.linewise,
        d.length > 1
      ), Zi.enterInsertMode(h, { head: p }, h.state.vim);
    },
    delete: function(h, c, d) {
      var p, m, v = h.state.vim;
      if (v.visualBlock) {
        m = h.getSelection();
        var S = Ia("", d.length);
        h.replaceSelections(S), p = nt(d[0].head, d[0].anchor);
      } else {
        var x = d[0].anchor, w = d[0].head;
        c.linewise && w.line != h.firstLine() && x.line == h.lastLine() && x.line == w.line - 1 && (x.line == h.firstLine() ? x.ch = 0 : x = new e(x.line - 1, Ze(h, x.line - 1))), m = h.getRange(x, w), h.replaceRange("", x, w), p = x, c.linewise && (p = Ht.moveToFirstNonWhiteSpaceCharacter(h, x));
      }
      return q.registerController.pushText(
        c.registerName,
        "delete",
        m,
        c.linewise,
        v.visualBlock
      ), it(h, p);
    },
    indent: function(h, c, d) {
      var p = h.state.vim, m = p.visualMode && c.repeat || 1;
      if (p.visualBlock) {
        for (var v = h.getOption("tabSize"), x = h.getOption("indentWithTabs") ? "	" : " ".repeat(v), w, S = d.length - 1; S >= 0; S--)
          if (w = nt(d[S].anchor, d[S].head), c.indentRight)
            h.replaceRange(x.repeat(m), w, w);
          else {
            for (var A = h.getLine(w.line), O = 0, L = 0; L < m; L++) {
              var D = A[w.ch + O];
              if (D == "	")
                O++;
              else if (D == " ") {
                O++;
                for (var B = 1; B < x.length && (D = A[w.ch + O], D === " "); B++)
                  O++;
              } else
                break;
            }
            h.replaceRange("", w, Ie(w, 0, O));
          }
        return w;
      } else if (h.indentMore)
        for (var L = 0; L < m; L++)
          c.indentRight ? h.indentMore() : h.indentLess();
      else {
        var T = d[0].anchor.line, W = p.visualBlock ? d[d.length - 1].anchor.line : d[0].head.line;
        c.linewise && W--;
        for (var S = T; S <= W; S++)
          for (var L = 0; L < m; L++)
            h.indentLine(S, c.indentRight);
      }
      return Ht.moveToFirstNonWhiteSpaceCharacter(h, d[0].anchor);
    },
    indentAuto: function(h, c, d) {
      return h.execCommand("indentAuto"), Ht.moveToFirstNonWhiteSpaceCharacter(h, d[0].anchor);
    },
    hardWrap: function(h, c, d, p) {
      if (h.hardWrap) {
        var m = d[0].anchor.line, v = d[0].head.line;
        c.linewise && v--;
        var x = h.hardWrap({ from: m, to: v });
        return x > m && c.linewise && x--, c.keepCursor ? p : new e(x, 0);
      }
    },
    changeCase: function(h, c, d, p, m) {
      for (var v = h.getSelections(), x = [], w = c.toLower, S = 0; S < v.length; S++) {
        var A = v[S], O = "";
        if (w === !0)
          O = A.toLowerCase();
        else if (w === !1)
          O = A.toUpperCase();
        else
          for (var L = 0; L < A.length; L++) {
            var D = A.charAt(L);
            O += H(D) ? D.toLowerCase() : D.toUpperCase();
          }
        x.push(O);
      }
      return h.replaceSelections(x), c.shouldMoveCursor ? m : !h.state.vim.visualMode && c.linewise && d[0].anchor.line + 1 == d[0].head.line ? Ht.moveToFirstNonWhiteSpaceCharacter(h, p) : c.linewise ? p : nt(d[0].anchor, d[0].head);
    },
    yank: function(h, c, d, p) {
      var m = h.state.vim, v = h.getSelection(), x = m.visualMode ? nt(m.sel.anchor, m.sel.head, d[0].head, d[0].anchor) : p;
      return q.registerController.pushText(
        c.registerName,
        "yank",
        v,
        c.linewise,
        m.visualBlock
      ), x;
    },
    rot13: function(h, c, d, p, m) {
      for (var v = h.getSelections(), x = [], w = 0; w < v.length; w++) {
        const S = v[w].split("").map((A) => {
          const O = A.charCodeAt(0);
          return O >= 65 && O <= 90 ? String.fromCharCode(65 + (O - 65 + 13) % 26) : O >= 97 && O <= 122 ? String.fromCharCode(97 + (O - 97 + 13) % 26) : A;
        }).join("");
        x.push(S);
      }
      return h.replaceSelections(x), c.shouldMoveCursor ? m : !h.state.vim.visualMode && c.linewise && d[0].anchor.line + 1 == d[0].head.line ? Ht.moveToFirstNonWhiteSpaceCharacter(h, p) : c.linewise ? p : nt(d[0].anchor, d[0].head);
    }
  };
  function Vp(h, c) {
    Ys[h] = c;
  }
  var Zi = {
    jumpListWalk: function(h, c, d) {
      if (!d.visualMode) {
        var p = c.repeat || 1, m = c.forward, v = q.jumpList, x = v.move(h, m ? p : -p), w = x ? x.find() : void 0;
        w = w || h.getCursor(), h.setCursor(w);
      }
    },
    scroll: function(h, c, d) {
      if (!d.visualMode) {
        var p = c.repeat || 1, m = h.defaultTextHeight(), v = h.getScrollInfo().top, x = m * p, w = c.forward ? v + x : v - x, S = Se(h.getCursor()), A = h.charCoords(S, "local");
        if (c.forward)
          w > A.top ? (S.line += (w - A.top) / m, S.line = Math.ceil(S.line), h.setCursor(S), A = h.charCoords(S, "local"), h.scrollTo(null, A.top)) : h.scrollTo(null, w);
        else {
          var O = w + h.getScrollInfo().clientHeight;
          O < A.bottom ? (S.line -= (A.bottom - O) / m, S.line = Math.floor(S.line), h.setCursor(S), A = h.charCoords(S, "local"), h.scrollTo(
            null,
            A.bottom - h.getScrollInfo().clientHeight
          )) : h.scrollTo(null, w);
        }
      }
    },
    scrollToCursor: function(h, c) {
      var d = h.getCursor().line, p = h.charCoords(new e(d, 0), "local"), m = h.getScrollInfo().clientHeight, v = p.top;
      switch (c.position) {
        case "center":
          v = p.bottom - m / 2;
          break;
        case "bottom":
          var x = new e(d, h.getLine(d).length - 1), w = h.charCoords(x, "local"), S = w.bottom - v;
          v = v - m + S;
          break;
      }
      h.scrollTo(null, v);
    },
    replayMacro: function(h, c, d) {
      var p = c.selectedCharacter || "", m = c.repeat || 1, v = q.macroModeState;
      for (p == "@" ? p = v.latestRegister || "" : v.latestRegister = p; m--; )
        Sg(h, d, v, p);
    },
    enterMacroRecordMode: function(h, c) {
      var d = q.macroModeState, p = c.selectedCharacter;
      q.registerController.isValidRegister(p) && d.enterMacroRecordMode(h, p);
    },
    toggleOverwrite: function(h) {
      h.state.overwrite ? (h.toggleOverwrite(!1), h.setOption("keyMap", "vim-insert"), n.signal(h, "vim-mode-change", { mode: "insert" })) : (h.toggleOverwrite(!0), h.setOption("keyMap", "vim-replace"), n.signal(h, "vim-mode-change", { mode: "replace" }));
    },
    enterInsertMode: function(h, c, d) {
      if (!h.getOption("readOnly")) {
        d.insertMode = !0, d.insertModeRepeat = c && c.repeat || 1;
        var p = c ? c.insertAt : null, m = d.sel, v = c.head || h.getCursor("head"), x = h.listSelections().length;
        if (p == "eol")
          v = new e(v.line, Ze(h, v.line));
        else if (p == "bol")
          v = new e(v.line, 0);
        else if (p == "charAfter") {
          var w = t(h, v, Ie(v, 0, 1));
          v = w.end;
        } else if (p == "firstNonBlank") {
          var w = t(h, v, Ht.moveToFirstNonWhiteSpaceCharacter(h, v));
          v = w.end;
        } else if (p == "startOfSelectedArea") {
          if (!d.visualMode)
            return;
          d.visualBlock ? (v = new e(
            Math.min(m.head.line, m.anchor.line),
            Math.min(m.head.ch, m.anchor.ch)
          ), x = Math.abs(m.head.line - m.anchor.line) + 1) : m.head.line < m.anchor.line ? v = m.head : v = new e(m.anchor.line, 0);
        } else if (p == "endOfSelectedArea") {
          if (!d.visualMode)
            return;
          d.visualBlock ? (v = new e(
            Math.min(m.head.line, m.anchor.line),
            Math.max(m.head.ch, m.anchor.ch) + 1
          ), x = Math.abs(m.head.line - m.anchor.line) + 1) : m.head.line >= m.anchor.line ? v = Ie(m.head, 0, 1) : v = new e(m.anchor.line, 0);
        } else if (p == "inplace") {
          if (d.visualMode)
            return;
        } else p == "lastEdit" && (v = th(h) || v);
        h.setOption("disableInput", !1), c && c.replace ? (h.toggleOverwrite(!0), h.setOption("keyMap", "vim-replace"), n.signal(h, "vim-mode-change", { mode: "replace" })) : (h.toggleOverwrite(!1), h.setOption("keyMap", "vim-insert"), n.signal(h, "vim-mode-change", { mode: "insert" })), q.macroModeState.isPlaying || (h.on("change", nh), d.insertEnd && d.insertEnd.clear(), d.insertEnd = h.setBookmark(v, { insertLeft: !0 }), n.on(h.getInputField(), "keydown", oh)), d.visualMode && zt(h), Qa(h, v, x);
      }
    },
    toggleVisualMode: function(h, c, d) {
      var p = c.repeat, m = h.getCursor(), v;
      if (d.visualMode)
        d.visualLine != !!c.linewise || d.visualBlock != !!c.blockwise ? (d.visualLine = !!c.linewise, d.visualBlock = !!c.blockwise, n.signal(h, "vim-mode-change", { mode: "visual", subMode: d.visualLine ? "linewise" : d.visualBlock ? "blockwise" : "" }), Ji(h)) : zt(h);
      else {
        d.visualMode = !0, d.visualLine = !!c.linewise, d.visualBlock = !!c.blockwise, v = it(
          h,
          new e(m.line, m.ch + p - 1)
        );
        var x = t(h, m, v);
        d.sel = {
          anchor: x.start,
          head: x.end
        }, n.signal(h, "vim-mode-change", { mode: "visual", subMode: d.visualLine ? "linewise" : d.visualBlock ? "blockwise" : "" }), Ji(h), ii(h, d, "<", nt(m, v)), ii(h, d, ">", ui(m, v));
      }
    },
    reselectLastSelection: function(h, c, d) {
      var p = d.lastSelection;
      if (d.visualMode && Va(h, d), p) {
        var m = p.anchorMark.find(), v = p.headMark.find();
        if (!m || !v)
          return;
        d.sel = {
          anchor: m,
          head: v
        }, d.visualMode = !0, d.visualLine = p.visualLine, d.visualBlock = p.visualBlock, Ji(h), ii(h, d, "<", nt(m, v)), ii(h, d, ">", ui(m, v)), n.signal(h, "vim-mode-change", {
          mode: "visual",
          subMode: d.visualLine ? "linewise" : d.visualBlock ? "blockwise" : ""
        });
      }
    },
    joinLines: function(h, c, d) {
      var p, m;
      if (d.visualMode) {
        if (p = h.getCursor("anchor"), m = h.getCursor("head"), Re(m, p)) {
          var v = m;
          m = p, p = v;
        }
        m.ch = Ze(h, m.line) - 1;
      } else {
        var x = Math.max(c.repeat, 2);
        p = h.getCursor(), m = it(h, new e(
          p.line + x - 1,
          1 / 0
        ));
      }
      for (var w = 0, S = p.line; S < m.line; S++) {
        w = Ze(h, p.line);
        var A = "", O = 0;
        if (!c.keepSpaces) {
          var L = h.getLine(p.line + 1);
          O = L.search(/\S/), O == -1 ? O = L.length : A = " ";
        }
        h.replaceRange(
          A,
          new e(p.line, w),
          new e(p.line + 1, O)
        );
      }
      var D = it(h, new e(p.line, w));
      d.visualMode && zt(h, !1), h.setCursor(D);
    },
    newLineAndEnterInsertMode: function(h, c, d) {
      d.insertMode = !0;
      var p = Se(h.getCursor());
      if (p.line === h.firstLine() && !c.after)
        h.replaceRange(`
`, new e(h.firstLine(), 0)), h.setCursor(h.firstLine(), 0);
      else {
        p.line = c.after ? p.line : p.line - 1, p.ch = Ze(h, p.line), h.setCursor(p);
        var m = n.commands.newlineAndIndentContinueComment || n.commands.newlineAndIndent;
        m(h);
      }
      this.enterInsertMode(h, { repeat: c.repeat }, d);
    },
    paste: function(h, c, d) {
      var p = q.registerController.getRegister(
        c.registerName
      );
      if (c.registerName === "+")
        navigator.clipboard.readText().then((v) => {
          this.continuePaste(h, c, d, v, p);
        });
      else {
        var m = p.toString();
        this.continuePaste(h, c, d, m, p);
      }
    },
    continuePaste: function(h, c, d, p, m) {
      var v = Se(h.getCursor());
      if (p) {
        if (c.matchIndent) {
          var x = h.getOption("tabSize"), w = function(_e) {
            var wt = _e.split("	").length - 1, Li = _e.split(" ").length - 1;
            return wt * x + Li * 1;
          }, S = h.getLine(h.getCursor().line), A = w(S.match(/^\s*/)[0]), O = p.replace(/\n$/, ""), L = p !== O, D = w(p.match(/^\s*/)[0]), p = O.replace(/^\s*/gm, function(_e) {
            var wt = A + (w(_e) - D);
            if (wt < 0)
              return "";
            if (h.getOption("indentWithTabs")) {
              var Li = Math.floor(wt / x);
              return Array(Li + 1).join("	");
            } else
              return Array(wt + 1).join(" ");
          });
          p += L ? `
` : "";
        }
        c.repeat > 1 && (p = Array(c.repeat + 1).join(p));
        var B = m.linewise, T = m.blockwise, W = T ? p.split(`
`) : void 0;
        if (W) {
          B && W.pop();
          for (var V = 0; V < W.length; V++)
            W[V] = W[V] == "" ? " " : W[V];
          v.ch += c.after ? 1 : 0, v.ch = Math.min(Ze(h, v.line), v.ch);
        } else B ? d.visualMode ? p = d.visualLine ? p.slice(0, -1) : `
` + p.slice(0, p.length - 1) + `
` : c.after ? (p = `
` + p.slice(0, p.length - 1), v.ch = Ze(h, v.line)) : v.ch = 0 : v.ch += c.after ? 1 : 0;
        var X;
        if (d.visualMode) {
          d.lastPastedText = p;
          var K, te = Up(h), ae = te[0], re = te[1], Ae = h.getSelection(), Pe = h.listSelections(), ue = new Array(Pe.length).join("1").split("1");
          d.lastSelection && (K = d.lastSelection.headMark.find()), q.registerController.unnamedRegister.setText(Ae), T ? (h.replaceSelections(ue), re = new e(ae.line + p.length - 1, ae.ch), h.setCursor(ae), Wa(h, re), h.replaceSelections(p), X = ae) : d.visualBlock ? (h.replaceSelections(ue), h.setCursor(ae), h.replaceRange(p, ae, ae), X = ae) : (h.replaceRange(p, ae, re), X = h.posFromIndex(h.indexFromPos(ae) + p.length - 1)), K && (d.lastSelection.headMark = h.setBookmark(K)), B && (X.ch = 0);
        } else if (T && W) {
          h.setCursor(v);
          for (var V = 0; V < W.length; V++) {
            var J = v.line + V;
            J > h.lastLine() && h.replaceRange(`
`, new e(J, 0));
            var Ce = Ze(h, J);
            Ce < v.ch && _p(h, J, v.ch);
          }
          h.setCursor(v), Wa(h, new e(v.line + W.length - 1, v.ch)), h.replaceSelections(W), X = v;
        } else if (h.replaceRange(p, v), B) {
          var J = c.after ? v.line + 1 : v.line;
          X = new e(J, $t(h.getLine(J)));
        } else
          X = Se(v), /\n/.test(p) || (X.ch += p.length - (c.after ? 1 : 0));
        d.visualMode && zt(h, !1), h.setCursor(X);
      }
    },
    undo: function(h, c) {
      h.operation(function() {
        Na(h, n.commands.undo, c.repeat)(), h.setCursor(it(h, h.getCursor("start")));
      });
    },
    redo: function(h, c) {
      Na(h, n.commands.redo, c.repeat)();
    },
    setRegister: function(h, c, d) {
      d.inputState.registerName = c.selectedCharacter;
    },
    insertRegister: function(h, c, d) {
      var p = c.selectedCharacter, m = q.registerController.getRegister(p), v = m && m.toString();
      v && h.replaceSelection(v);
    },
    oneNormalCommand: function(h, c, d) {
      di(h, !0), d.insertModeReturn = !0, n.on(h, "vim-command-done", function p() {
        d.visualMode || (d.insertModeReturn && (d.insertModeReturn = !1, d.insertMode || Zi.enterInsertMode(h, {}, d)), n.off(h, "vim-command-done", p));
      });
    },
    setMark: function(h, c, d) {
      var p = c.selectedCharacter;
      p && ii(h, d, p, h.getCursor());
    },
    replace: function(h, c, d) {
      var p = c.selectedCharacter || "", m = h.getCursor(), v, x, w = h.listSelections();
      if (d.visualMode)
        m = h.getCursor("start"), x = h.getCursor("end");
      else {
        var S = h.getLine(m.line);
        v = m.ch + c.repeat, v > S.length && (v = S.length), x = new e(m.line, v);
      }
      var A = t(h, m, x);
      if (m = A.start, x = A.end, p == `
`)
        d.visualMode || h.replaceRange("", m, x), (n.commands.newlineAndIndentContinueComment || n.commands.newlineAndIndent)(h);
      else {
        var O = h.getRange(m, x);
        if (O = O.replace(/[\uD800-\uDBFF][\uDC00-\uDFFF]/g, p), O = O.replace(/[^\n]/g, p), d.visualBlock) {
          var L = new Array(h.getOption("tabSize") + 1).join(" ");
          O = h.getSelection(), O = O.replace(/[\uD800-\uDBFF][\uDC00-\uDFFF]/g, p);
          var D = O.replace(/\t/g, L).replace(/[^\n]/g, p).split(`
`);
          h.replaceSelections(D);
        } else
          h.replaceRange(O, m, x);
        d.visualMode ? (m = Re(w[0].anchor, w[0].head) ? w[0].anchor : w[0].head, h.setCursor(m), zt(h, !1)) : h.setCursor(Ie(x, 0, -1));
      }
    },
    incrementNumberToken: function(h, c) {
      for (var d = h.getCursor(), p = h.getLine(d.line), m = /(-?)(?:(0x)([\da-f]+)|(0b|0|)(\d+))/gi, v, x, w, S; (v = m.exec(p)) !== null && (x = v.index, w = x + v[0].length, !(d.ch < w)); )
        ;
      if (!(!c.backtrack && w <= d.ch)) {
        if (v) {
          var A = v[2] || v[4], O = v[3] || v[5], L = c.increase ? 1 : -1, D = { "0b": 2, 0: 8, "": 10, "0x": 16 }[A.toLowerCase()], B = parseInt(v[1] + O, D) + L * c.repeat;
          S = B.toString(D);
          var T = A ? new Array(O.length - S.length + 1 + v[1].length).join("0") : "";
          S.charAt(0) === "-" ? S = "-" + A + T + S.substr(1) : S = A + T + S;
          var W = new e(d.line, x), V = new e(d.line, w);
          h.replaceRange(S, W, V);
        } else
          return;
        h.setCursor(new e(d.line, x + S.length - 1));
      }
    },
    repeatLastEdit: function(h, c, d) {
      var p = d.lastEditInputState;
      if (p) {
        var m = c.repeat;
        m && c.repeatIsExplicit ? p.repeatOverride = m : m = p.repeatOverride || m, lh(
          h,
          d,
          m,
          !1
          /** repeatForInsert */
        );
      }
    },
    indent: function(h, c) {
      h.indentLine(h.getCursor().line, c.indentRight);
    },
    exitInsertMode: function(h, c) {
      di(h);
    }
  };
  function Hp(h, c) {
    Zi[h] = c;
  }
  function it(h, c, d) {
    var p = h.state.vim, m = p.insertMode || p.visualMode, v = Math.min(Math.max(h.firstLine(), c.line), h.lastLine()), x = h.getLine(v), w = x.length - 1 + +!!m, S = Math.min(Math.max(0, c.ch), w), A = x.charCodeAt(S);
    if (56320 <= A && A <= 57343) {
      var O = 1;
      d && d.line == v && d.ch > S && (O = -1), S += O, S > w && (S -= 2);
    }
    return new e(v, S);
  }
  function yr(h) {
    var c = (
      /**@type{typeof args}*/
      {}
    );
    for (var d in h)
      Object.prototype.hasOwnProperty.call(h, d) && (c[d] = h[d]);
    return (
      /**@type{typeof args}*/
      c
    );
  }
  function Ie(h, c, d) {
    return typeof c == "object" && (d = c.ch, c = c.line), new e(h.line + c, h.ch + d);
  }
  function zp(h, c, d, p) {
    p.operator && (d = "operatorPending");
    for (var m, v = [], x = [], w = bt ? c.length - s : 0, S = w; S < c.length; S++) {
      var A = c[S];
      d == "insert" && A.context != "insert" || A.context && A.context != d || p.operator && A.type == "action" || !(m = $p(h, A.keys)) || (m == "partial" && v.push(A), m == "full" && x.push(A));
    }
    return {
      partial: v,
      full: x
    };
  }
  function $p(h, c) {
    const d = c.slice(-11) == "<character>", p = c.slice(-10) == "<register>";
    if (d || p) {
      var m = c.length - (d ? 11 : 10), v = h.slice(0, m), x = c.slice(0, m);
      return v == x && h.length > m ? "full" : x.indexOf(v) == 0 ? "partial" : !1;
    } else
      return h == c ? "full" : c.indexOf(h) == 0 ? "partial" : !1;
  }
  function qp(h) {
    var c = /^.*(<[^>]+>)$/.exec(h), d = c ? c[1] : h.slice(-1);
    if (d.length > 1)
      switch (d) {
        case "<CR>":
        case "<S-CR>":
          d = `
`;
          break;
        case "<Space>":
        case "<S-Space>":
          d = " ";
          break;
        default:
          d = "";
          break;
      }
    return d;
  }
  function Na(h, c, d) {
    return function() {
      for (var p = 0; p < d; p++)
        c(h);
    };
  }
  function Se(h) {
    return new e(h.line, h.ch);
  }
  function xt(h, c) {
    return h.ch == c.ch && h.line == c.line;
  }
  function Re(h, c) {
    return h.line < c.line || h.line == c.line && h.ch < c.ch;
  }
  function nt(h, c) {
    return arguments.length > 2 && (c = nt.apply(void 0, Array.prototype.slice.call(arguments, 1))), Re(h, c) ? h : c;
  }
  function ui(h, c) {
    return arguments.length > 2 && (c = ui.apply(void 0, Array.prototype.slice.call(arguments, 1))), Re(h, c) ? c : h;
  }
  function Fa(h, c, d) {
    var p = Re(h, c), m = Re(c, d);
    return p && m;
  }
  function Ze(h, c) {
    return h.getLine(c).length;
  }
  function Gs(h) {
    return h.trim ? h.trim() : h.replace(/^\s+|\s+$/g, "");
  }
  function Kp(h) {
    return h.replace(/([.?*+$\[\]\/\\(){}|\-])/g, "\\$1");
  }
  function _p(h, c, d) {
    var p = Ze(h, c), m = new Array(d - p + 1).join(" ");
    h.setCursor(new e(c, p)), h.replaceRange(m, h.getCursor());
  }
  function Wa(h, c) {
    var d = [], p = h.listSelections(), m = Se(h.clipPos(c)), v = !xt(c, m), x = h.getCursor("head"), w = jp(p, x), S = xt(p[w].head, p[w].anchor), A = p.length - 1, O = A - w > w ? A : 0, L = p[O].anchor, D = Math.min(L.line, m.line), B = Math.max(L.line, m.line), T = L.ch, W = m.ch, V = p[O].head.ch - T, X = W - T;
    V > 0 && X <= 0 ? (T++, v || W--) : V < 0 && X >= 0 ? (T--, S || W++) : V < 0 && X == -1 && (T--, W++);
    for (var K = D; K <= B; K++) {
      var te = { anchor: new e(K, T), head: new e(K, W) };
      d.push(te);
    }
    return h.setSelections(d), c.ch = W, L.ch = T, L;
  }
  function Qa(h, c, d) {
    for (var p = [], m = 0; m < d; m++) {
      var v = Ie(c, m, 0);
      p.push({ anchor: v, head: v });
    }
    h.setSelections(p, 0);
  }
  function jp(h, c, d) {
    for (var p = 0; p < h.length; p++) {
      var m = xt(h[p].anchor, c), v = xt(h[p].head, c);
      if (m || v)
        return p;
    }
    return -1;
  }
  function Up(h, c) {
    var d = h.listSelections(), p = d[0], m = d[d.length - 1], v = Re(p.anchor, p.head) ? p.anchor : p.head, x = Re(m.anchor, m.head) ? m.head : m.anchor;
    return [v, x];
  }
  function Va(h, c) {
    var d = c.sel.anchor, p = c.sel.head;
    c.lastPastedText && (p = h.posFromIndex(h.indexFromPos(d) + c.lastPastedText.length), c.lastPastedText = void 0), c.lastSelection = {
      anchorMark: h.setBookmark(d),
      headMark: h.setBookmark(p),
      anchor: Se(d),
      head: Se(p),
      visualMode: c.visualMode,
      visualLine: c.visualLine,
      visualBlock: c.visualBlock
    };
  }
  function Xp(h, c, d, p) {
    var m = h.state.vim.sel, v = p ? c : m.head, x = p ? c : m.anchor, w;
    return Re(d, c) && (w = d, d = c, c = w), Re(v, x) ? (v = nt(c, v), x = ui(x, d)) : (x = nt(c, x), v = ui(v, d), v = Ie(v, 0, -1), v.ch == -1 && v.line != h.firstLine() && (v = new e(v.line - 1, Ze(h, v.line - 1)))), [x, v];
  }
  function Ji(h, c, d) {
    var p = h.state.vim;
    c = c || p.sel, d || (d = p.visualLine ? "line" : p.visualBlock ? "block" : "char");
    var m = Zs(h, c, d);
    h.setSelections(m.ranges, m.primary);
  }
  function Zs(h, c, d, p) {
    var m = Se(c.head), v = Se(c.anchor);
    if (d == "char") {
      var x = !p && !Re(c.head, c.anchor) ? 1 : 0, w = Re(c.head, c.anchor) ? 1 : 0;
      return m = Ie(c.head, 0, x), v = Ie(c.anchor, 0, w), {
        ranges: [{ anchor: v, head: m }],
        primary: 0
      };
    } else if (d == "line") {
      if (Re(c.head, c.anchor))
        m.ch = 0, v.ch = Ze(h, v.line);
      else {
        v.ch = 0;
        var S = h.lastLine();
        m.line > S && (m.line = S), m.ch = Ze(h, m.line);
      }
      return {
        ranges: [{ anchor: v, head: m }],
        primary: 0
      };
    } else if (d == "block") {
      var A = Math.min(v.line, m.line), O = v.ch, L = Math.max(v.line, m.line), D = m.ch;
      O < D ? D += 1 : O += 1;
      for (var B = L - A + 1, T = m.line == A ? 0 : B - 1, W = [], V = 0; V < B; V++)
        W.push({
          anchor: new e(A + V, O),
          head: new e(A + V, D)
        });
      return {
        ranges: W,
        primary: T
      };
    }
    throw "never happens";
  }
  function Yp(h) {
    var c = h.getCursor("head");
    return h.getSelection().length == 1 && (c = nt(c, h.getCursor("anchor"))), c;
  }
  function zt(h, c) {
    var d = h.state.vim;
    c !== !1 && h.setCursor(it(h, d.sel.head)), Va(h, d), d.visualMode = !1, d.visualLine = !1, d.visualBlock = !1, d.insertMode || n.signal(h, "vim-mode-change", { mode: "normal" });
  }
  function Gp(h, c, d) {
    var p = h.getRange(c, d);
    if (/\n\s*$/.test(p)) {
      var m = p.split(`
`);
      m.pop();
      for (var v = m.pop(); m.length > 0 && v && Q(v); v = m.pop())
        d.line--, d.ch = 0;
      v ? (d.line--, d.ch = Ze(h, d.line)) : d.ch = 0;
    }
  }
  function Zp(h, c, d) {
    c.ch = 0, d.ch = 0, d.line++;
  }
  function $t(h) {
    if (!h)
      return 0;
    var c = h.search(/\S/);
    return c == -1 ? h.length : c;
  }
  function Js(h, { inclusive: c, innerWord: d, bigWord: p, noSymbol: m, multiline: v }, x) {
    var w = x || Yp(h), S = h.getLine(w.line), A = S, O = w.line, L = O, D = w.ch, B, T = m ? y[0] : b[0];
    if (d && /\s/.test(S.charAt(D)))
      T = function(ae) {
        return /\s/.test(ae);
      };
    else {
      for (; !T(S.charAt(D)); )
        if (D++, D >= S.length) {
          if (!v) return null;
          D--, B = qa(h, w, !0, p, !0);
          break;
        }
      p ? T = b[0] : (T = y[0], T(S.charAt(D)) || (T = y[1]));
    }
    for (var W = D, V = D; T(S.charAt(V)) && V >= 0; )
      V--;
    if (V++, B)
      W = B.to, L = B.line, A = h.getLine(L), !A && W == 0 && W++;
    else
      for (; T(S.charAt(W)) && W < S.length; )
        W++;
    if (c) {
      var X = W, K = w.ch <= V && /\s/.test(S.charAt(w.ch));
      if (!K)
        for (; /\s/.test(A.charAt(W)) && W < A.length; )
          W++;
      if (X == W || K) {
        for (var te = V; /\s/.test(S.charAt(V - 1)) && V > 0; )
          V--;
        !V && !K && (V = te);
      }
    }
    return { start: new e(O, V), end: new e(L, W) };
  }
  function Jp(h, c, d) {
    var p = c;
    if (!n.findMatchingTag || !n.findEnclosingTag)
      return { start: p, end: p };
    var m = n.findMatchingTag(h, c) || n.findEnclosingTag(h, c);
    return !m || !m.open || !m.close ? { start: p, end: p } : d ? { start: m.open.from, end: m.close.to } : { start: m.open.to, end: m.close.from };
  }
  function Ha(h, c, d) {
    xt(c, d) || q.jumpList.add(h, c, d);
  }
  function za(h, c) {
    q.lastCharacterSearch.increment = h, q.lastCharacterSearch.forward = c.forward, q.lastCharacterSearch.selectedCharacter = c.selectedCharacter;
  }
  var eg = {
    "(": "bracket",
    ")": "bracket",
    "{": "bracket",
    "}": "bracket",
    "[": "section",
    "]": "section",
    "*": "comment",
    "/": "comment",
    m: "method",
    M: "method",
    "#": "preprocess"
  }, $a = {
    bracket: {
      isComplete: function(h) {
        if (h.nextCh === h.symb) {
          if (h.depth++, h.depth >= 1) return !0;
        } else h.nextCh === h.reverseSymb && h.depth--;
        return !1;
      }
    },
    section: {
      init: function(h) {
        h.curMoveThrough = !0, h.symb = (h.forward ? "]" : "[") === h.symb ? "{" : "}";
      },
      isComplete: function(h) {
        return h.index === 0 && h.nextCh === h.symb;
      }
    },
    comment: {
      isComplete: function(h) {
        var c = h.lastCh === "*" && h.nextCh === "/";
        return h.lastCh = h.nextCh, c;
      }
    },
    // TODO: The original Vim implementation only operates on level 1 and 2.
    // The current implementation doesn't check for code block level and
    // therefore it operates on any levels.
    method: {
      init: function(h) {
        h.symb = h.symb === "m" ? "{" : "}", h.reverseSymb = h.symb === "{" ? "}" : "{";
      },
      isComplete: function(h) {
        return h.nextCh === h.symb;
      }
    },
    preprocess: {
      init: function(h) {
        h.index = 0;
      },
      isComplete: function(h) {
        if (h.nextCh === "#") {
          var c = h.lineText.match(/^#(\w+)/)?.[1];
          if (c === "endif") {
            if (h.forward && h.depth === 0)
              return !0;
            h.depth++;
          } else if (c === "if") {
            if (!h.forward && h.depth === 0)
              return !0;
            h.depth--;
          }
          if (c === "else" && h.depth === 0) return !0;
        }
        return !1;
      }
    }
  };
  function tg(h, c, d, p) {
    var m = Se(h.getCursor()), v = d ? 1 : -1, x = d ? h.lineCount() : -1, w = m.ch, S = m.line, A = h.getLine(S), O = {
      lineText: A,
      nextCh: A.charAt(w),
      lastCh: null,
      index: w,
      symb: p,
      reverseSymb: (d ? { ")": "(", "}": "{" } : { "(": ")", "{": "}" })[p],
      forward: d,
      depth: 0,
      curMoveThrough: !1
    }, L = eg[p];
    if (!L) return m;
    var D = $a[L].init, B = $a[L].isComplete;
    for (D && D(O); S !== x && c; ) {
      if (O.index += v, O.nextCh = O.lineText.charAt(O.index), !O.nextCh) {
        if (S += v, O.lineText = h.getLine(S) || "", v > 0)
          O.index = 0;
        else {
          var T = O.lineText.length;
          O.index = T > 0 ? T - 1 : 0;
        }
        O.nextCh = O.lineText.charAt(O.index);
      }
      B(O) && (m.line = S, m.ch = O.index, c--);
    }
    return O.nextCh || O.curMoveThrough ? new e(S, O.index) : m;
  }
  function qa(h, c, d, p, m) {
    var v = c.line, x = c.ch, w = h.getLine(v), S = d ? 1 : -1, A = p ? b : y;
    if (m && w == "") {
      if (v += S, w = h.getLine(v), !I(h, v))
        return null;
      x = d ? 0 : w.length;
    }
    for (; ; ) {
      if (m && w == "")
        return { from: 0, to: 0, line: v };
      for (var O = S > 0 ? w.length : -1, L = O, D = O; x != O; ) {
        for (var B = !1, T = 0; T < A.length && !B; ++T)
          if (A[T](w.charAt(x))) {
            for (L = x; x != O && A[T](w.charAt(x)); )
              x += S;
            if (D = x, B = L != D, L == c.ch && v == c.line && D == L + S)
              continue;
            return {
              from: Math.min(L, D + 1),
              to: Math.max(L, D),
              line: v
            };
          }
        B || (x += S);
      }
      if (v += S, !I(h, v))
        return null;
      w = h.getLine(v), x = S > 0 ? 0 : w.length;
    }
  }
  function ig(h, c, d, p, m, v) {
    var x = Se(c), w = [];
    (p && !m || !p && m) && d++;
    for (var S = !(p && m), A = 0; A < d; A++) {
      var O = qa(h, c, p, v, S);
      if (!O) {
        var L = Ze(h, h.lastLine());
        w.push(p ? { line: h.lastLine(), from: L, to: L } : { line: 0, from: 0, to: 0 });
        break;
      }
      w.push(O), c = new e(O.line, p ? O.to - 1 : O.from);
    }
    var D = w.length != d, B = w[0], T = w.pop();
    return p && !m ? (!D && (B.from != x.ch || B.line != x.line) && (T = w.pop()), T && new e(T.line, T.from)) : p && m ? T && new e(T.line, T.to - 1) : !p && m ? (!D && (B.to != x.ch || B.line != x.line) && (T = w.pop()), T && new e(T.line, T.to)) : T && new e(T.line, T.from);
  }
  function Ka(h, c, d, p, m) {
    var v = c, x = new e(v.line + d.repeat - 1, 1 / 0), w = h.clipPos(x);
    return w.ch--, m || (p.lastHPos = 1 / 0, p.lastHSPos = h.charCoords(w, "div").left), x;
  }
  function eo(h, c, d, p, m) {
    if (p) {
      for (var v = m || h.getCursor(), x = v.ch, w, S = 0; S < c; S++) {
        var A = h.getLine(v.line);
        if (w = rg(x, A, p, d), w == -1)
          return;
        x = w;
      }
      if (w != null)
        return new e(h.getCursor().line, w);
    }
  }
  function ng(h, c) {
    var d = h.getCursor().line;
    return it(h, new e(d, c - 1));
  }
  function ii(h, c, d, p) {
    !le(d, k) && !M.test(d) || (c.marks[d] && c.marks[d].clear(), c.marks[d] = h.setBookmark(p));
  }
  function rg(h, c, d, p, m) {
    var v;
    return p ? v = c.indexOf(d, h + 1) : v = c.lastIndexOf(d, h - 1), v;
  }
  function _a(h, c, d, p, m) {
    var v = c.line, x = h.firstLine(), w = h.lastLine(), S, A, O = v;
    function L(V) {
      return !h.getLine(V);
    }
    function D(V, X, K) {
      return K ? L(V) != L(V + X) : !L(V) && L(V + X);
    }
    if (p) {
      for (; x <= O && O <= w && d > 0; )
        D(O, p) && d--, O += p;
      return { start: new e(O, 0), end: c };
    }
    var B = h.state.vim;
    if (B.visualLine && D(v, 1, !0)) {
      var T = B.sel.anchor;
      D(T.line, -1, !0) && (!m || T.line != v) && (v += 1);
    }
    var W = L(v);
    for (O = v; O <= w && d; O++)
      D(O, 1, !0) && (!m || L(O) != W) && d--;
    for (A = new e(O, 0), O > w && !W ? W = !0 : m = !1, O = v; O > x && !((!m || L(O) == W || O == v) && D(O, -1, !0)); O--)
      ;
    return S = new e(O, 0), { start: S, end: A };
  }
  function ja(h, c, d, p, m) {
    function v(A) {
      A.line !== null && (A.pos + A.dir < 0 || A.pos + A.dir >= A.line.length ? A.line = null : A.pos += A.dir);
    }
    function x(A, O, L, D) {
      var B = A.getLine(O), T = {
        line: B,
        ln: O,
        pos: L,
        dir: D
      };
      if (T.line === "")
        return { ln: T.ln, pos: T.pos };
      var W = T.pos;
      for (v(T); T.line !== null; ) {
        if (W = T.pos, Z(T.line[T.pos]))
          if (m) {
            for (v(T); T.line !== null && Q(T.line[T.pos]); )
              W = T.pos, v(T);
            return { ln: T.ln, pos: W + 1 };
          } else
            return { ln: T.ln, pos: T.pos + 1 };
        v(T);
      }
      return { ln: T.ln, pos: W + 1 };
    }
    function w(A, O, L, D) {
      var B = A.getLine(O), T = {
        line: B,
        ln: O,
        pos: L,
        dir: D
      };
      if (T.line === "")
        return { ln: T.ln, pos: T.pos };
      var W = T.pos;
      for (v(T); T.line !== null; ) {
        if (!Q(T.line[T.pos]) && !Z(T.line[T.pos]))
          W = T.pos;
        else if (Z(T.line[T.pos]))
          return m ? Q(T.line[T.pos + 1]) ? { ln: T.ln, pos: T.pos + 1 } : { ln: T.ln, pos: W } : { ln: T.ln, pos: W };
        v(T);
      }
      return T.line = B, m && Q(T.line[T.pos]) ? { ln: T.ln, pos: T.pos } : { ln: T.ln, pos: W };
    }
    for (var S = {
      ln: c.line,
      pos: c.ch
    }; d > 0; )
      p < 0 ? S = w(h, S.ln, S.pos, p) : S = x(h, S.ln, S.pos, p), d--;
    return new e(S.ln, S.pos);
  }
  function sg(h, c, d, p) {
    function m(S, A) {
      if (A.line !== null)
        if (A.pos + A.dir < 0 || A.pos + A.dir >= A.line.length) {
          if (A.ln += A.dir, !I(S, A.ln)) {
            A.line = null;
            return;
          }
          A.line = S.getLine(A.ln), A.pos = A.dir > 0 ? 0 : A.line.length - 1;
        } else
          A.pos += A.dir;
    }
    function v(S, A, O, L) {
      var V = S.getLine(A), D = V === "", B = {
        line: V,
        ln: A,
        pos: O,
        dir: L
      }, T = {
        ln: B.ln,
        pos: B.pos
      }, W = B.line === "";
      for (m(S, B); B.line !== null; ) {
        if (T.ln = B.ln, T.pos = B.pos, B.line === "" && !W)
          return { ln: B.ln, pos: B.pos };
        if (D && B.line !== "" && !Q(B.line[B.pos]))
          return { ln: B.ln, pos: B.pos };
        Z(B.line[B.pos]) && !D && (B.pos === B.line.length - 1 || Q(B.line[B.pos + 1])) && (D = !0), m(S, B);
      }
      var V = S.getLine(T.ln);
      T.pos = 0;
      for (var X = V.length - 1; X >= 0; --X)
        if (!Q(V[X])) {
          T.pos = X;
          break;
        }
      return T;
    }
    function x(S, A, O, L) {
      var V = S.getLine(A), D = {
        line: V,
        ln: A,
        pos: O,
        dir: L
      }, B = D.ln, T = null, W = D.line === "";
      for (m(S, D); D.line !== null; ) {
        if (D.line === "" && !W)
          return T !== null ? { ln: B, pos: T } : { ln: D.ln, pos: D.pos };
        if (Z(D.line[D.pos]) && T !== null && !(D.ln === B && D.pos + 1 === T))
          return { ln: B, pos: T };
        D.line !== "" && !Q(D.line[D.pos]) && (W = !1, B = D.ln, T = D.pos), m(S, D);
      }
      var V = S.getLine(B);
      T = 0;
      for (var X = 0; X < V.length; ++X)
        if (!Q(V[X])) {
          T = X;
          break;
        }
      return { ln: B, pos: T };
    }
    for (var w = {
      ln: c.line,
      pos: c.ch
    }; d > 0; )
      p < 0 ? w = x(h, w.ln, w.pos, p) : w = v(h, w.ln, w.pos, p), d--;
    return new e(w.ln, w.pos);
  }
  function Ua(h, c, d, p) {
    var m = c, v = {
      "(": /[()]/,
      ")": /[()]/,
      "[": /[[\]]/,
      "]": /[[\]]/,
      "{": /[{}]/,
      "}": /[{}]/,
      "<": /[<>]/,
      ">": /[<>]/
    }[d], x = {
      "(": "(",
      ")": "(",
      "[": "[",
      "]": "[",
      "{": "{",
      "}": "{",
      "<": "<",
      ">": "<"
    }[d], w = h.getLine(m.line).charAt(m.ch), S = w === x ? 1 : 0, A = h.scanForBracket(new e(m.line, m.ch + S), -1, void 0, { bracketRegex: v }), O = h.scanForBracket(new e(m.line, m.ch + S), 1, void 0, { bracketRegex: v });
    if (!A || !O) return null;
    var L = A.pos, D = O.pos;
    if (L.line == D.line && L.ch > D.ch || L.line > D.line) {
      var B = L;
      L = D, D = B;
    }
    return p ? D.ch += 1 : L.ch += 1, { start: L, end: D };
  }
  function og(h, c, d, p) {
    var m = Se(c), v = h.getLine(m.line), x = v.split(""), w, S, A, O, L = x.indexOf(d);
    if (m.ch < L)
      m.ch = L;
    else if (L < m.ch && x[m.ch] == d) {
      var D = /string/.test(h.getTokenTypeAt(Ie(c, 0, 1))), B = /string/.test(h.getTokenTypeAt(c)), T = D && !B;
      T || (S = m.ch, --m.ch);
    }
    if (x[m.ch] == d && !S)
      w = m.ch + 1;
    else
      for (A = m.ch; A > -1 && !w; A--)
        x[A] == d && (w = A + 1);
    if (w && !S)
      for (A = w, O = x.length; A < O && !S; A++)
        x[A] == d && (S = A);
    return !w || !S ? { start: m, end: m } : (p && (--w, ++S), {
      start: new e(m.line, w),
      end: new e(m.line, S)
    });
  }
  ee("pcre", !0, "boolean");
  class lg {
    constructor() {
      this.highlightTimeout;
    }
    getQuery() {
      return q.query;
    }
    setQuery(c) {
      q.query = c;
    }
    getOverlay() {
      return this.searchOverlay;
    }
    setOverlay(c) {
      this.searchOverlay = c;
    }
    isReversed() {
      return q.isReversed;
    }
    setReversed(c) {
      q.isReversed = c;
    }
    getScrollbarAnnotate() {
      return this.annotate;
    }
    setScrollbarAnnotate(c) {
      this.annotate = c;
    }
  }
  function Bt(h) {
    var c = h.state.vim;
    return c.searchState_ || (c.searchState_ = new lg());
  }
  function ag(h) {
    return Xa(h, "/");
  }
  function hg(h) {
    return Ya(h, "/");
  }
  function Xa(h, c) {
    var d = Ya(h, c) || [];
    if (!d.length) return [];
    var p = [];
    if (d[0] === 0) {
      for (var m = 0; m < d.length; m++)
        typeof d[m] == "number" && p.push(h.substring(d[m] + 1, d[m + 1]));
      return p;
    }
  }
  function Ya(h, c) {
    c || (c = "/");
    for (var d = !1, p = [], m = 0; m < h.length; m++) {
      var v = h.charAt(m);
      !d && v == c && p.push(m), d = !d && v == "\\";
    }
    return p;
  }
  function cg(h) {
    var c = {
      V: "|(){+?*.[$^",
      // verynomagic
      M: "|(){+?*.[",
      // nomagic
      m: "|(){+?",
      // magic
      v: "<>"
      // verymagic
    }, d = {
      ">": "(?<=[\\w])(?=[^\\w]|$)",
      "<": "(?<=[^\\w]|^)(?=[\\w])"
    }, p = c.m, m = h.replace(/\\.|[\[|(){+*?.$^<>]/g, function(x) {
      if (x[0] === "\\") {
        var w = x[1];
        return w === "}" || p.indexOf(w) != -1 ? w : w in c ? (p = c[w], "") : w in d ? d[w] : x;
      } else
        return p.indexOf(x) != -1 ? d[x] || "\\" + x : x;
    }), v = m.indexOf("\\zs");
    return v != -1 && (m = "(?<=" + m.slice(0, v) + ")" + m.slice(v + 3)), v = m.indexOf("\\ze"), v != -1 && (m = m.slice(0, v) + "(?=" + m.slice(v + 3) + ")"), m;
  }
  var Ga = { "\\n": `
`, "\\r": "\r", "\\t": "	" };
  function fg(h) {
    for (var c = !1, d = [], p = -1; p < h.length; p++) {
      var m = h.charAt(p) || "", v = h.charAt(p + 1) || "";
      Ga[m + v] ? (d.push(Ga[m + v]), p++) : c ? (d.push(m), c = !1) : m === "\\" ? (c = !0, F(v) || v === "$" ? d.push("$") : v !== "/" && v !== "\\" && d.push("\\")) : (m === "$" && d.push("$"), d.push(m), v === "/" && d.push("\\"));
    }
    return d.join("");
  }
  var Za = { "\\/": "/", "\\\\": "\\", "\\n": `
`, "\\r": "\r", "\\t": "	", "\\&": "&" };
  function ug(h) {
    for (var c = new n.StringStream(h), d = []; !c.eol(); ) {
      for (; c.peek() && c.peek() != "\\"; )
        d.push(c.next());
      var p = !1;
      for (var m in Za)
        if (c.match(m, !0)) {
          p = !0, d.push(Za[m]);
          break;
        }
      p || d.push(c.next());
    }
    return d.join("");
  }
  function Ja(h, c, d) {
    var p = q.registerController.getRegister("/");
    p.setText(h);
    var m = hg(h), v, x;
    if (!m.length)
      v = h;
    else {
      v = h.substring(0, m[0]);
      var w = h.substring(m[0]);
      x = w.indexOf("i") != -1;
    }
    if (!v)
      return null;
    ie("pcre") || (v = cg(v)), d && (c = /^[^A-Z]*$/.test(v));
    var S = new RegExp(
      v,
      c || x ? "im" : "m"
    );
    return S;
  }
  function Et(h) {
    typeof h == "string" && (h = document.createElement(h));
    for (var c = 1; c < arguments.length; c++) {
      var d = arguments[c];
      if (d)
        if (typeof d != "object" && (d = document.createTextNode(d)), d.nodeType) h.appendChild(d);
        else for (var p in d)
          Object.prototype.hasOwnProperty.call(d, p) && (p[0] === "$" ? h.style[p.slice(1)] = d[p] : typeof d[p] == "function" ? h[p] = d[p] : h.setAttribute(p, d[p]));
    }
    return h;
  }
  function de(h, c, d) {
    var p = Et("div", { $color: "red", $whiteSpace: "pre", class: "cm-vim-message" }, c);
    h.openNotification ? d ? (p = Et("div", {}, p, Et("div", {}, "Press ENTER or type command to continue")), h.state.closeVimNotification && h.state.closeVimNotification(), h.state.closeVimNotification = h.openNotification(p, { bottom: !0, duration: 0 })) : h.openNotification(p, { bottom: !0, duration: 15e3 }) : alert(p.innerText);
  }
  function dg(h, c) {
    return Et(
      "div",
      { $display: "flex", $flex: 1 },
      Et(
        "span",
        { $fontFamily: "monospace", $whiteSpace: "pre", $flex: 1, $display: "flex" },
        h,
        Et("input", {
          type: "text",
          autocorrect: "off",
          autocapitalize: "off",
          spellcheck: "false",
          $flex: 1
        })
      ),
      c && Et("span", { $color: "#888" }, c)
    );
  }
  function br(h, c) {
    if (Ke.length) {
      c.value || (c.value = ""), ve = c;
      return;
    }
    var d = dg(c.prefix, c.desc);
    if (h.openDialog)
      h.openDialog(d, c.onClose, {
        onKeyDown: c.onKeyDown,
        onKeyUp: c.onKeyUp,
        bottom: !0,
        selectValueOnOpen: !1,
        value: c.value
      });
    else {
      var p = "";
      typeof c.prefix != "string" && c.prefix && (p += c.prefix.textContent), c.desc && (p += " " + c.desc), c.onClose?.(prompt(p, ""));
    }
  }
  function pg(h, c) {
    return h instanceof RegExp && c instanceof RegExp ? h.flags == c.flags && h.source == c.source : !1;
  }
  function Ln(h, c, d, p) {
    if (c) {
      var m = Bt(h), v = Ja(c, !!d, !!p);
      if (v)
        return to(h, v), pg(v, m.getQuery()) || m.setQuery(v), v;
    }
  }
  function gg(h) {
    if (h.source.charAt(0) == "^")
      var c = !0;
    return {
      token: function(d) {
        if (c && !d.sol()) {
          d.skipToEnd();
          return;
        }
        var p = d.match(h, !1);
        if (p)
          return p[0].length == 0 ? (d.next(), "searching") : !d.sol() && (d.backUp(1), !h.exec(d.next() + p[0])) ? (d.next(), null) : (d.match(h), "searching");
        for (; !d.eol() && (d.next(), !d.match(h, !1)); )
          ;
      },
      query: h
    };
  }
  var Dn = 0;
  function to(h, c) {
    clearTimeout(Dn);
    var d = Bt(h);
    d.highlightTimeout = Dn, Dn = setTimeout(function() {
      if (h.state.vim) {
        var p = Bt(h);
        p.highlightTimeout = void 0;
        var m = p.getOverlay();
        (!m || c != m.query) && (m && h.removeOverlay(m), m = gg(c), h.addOverlay(m), h.showMatchesOnScrollbar && (p.getScrollbarAnnotate() && p.getScrollbarAnnotate().clear(), p.setScrollbarAnnotate(h.showMatchesOnScrollbar(c))), p.setOverlay(m));
      }
    }, 50);
  }
  function eh(h, c, d, p) {
    return h.operation(function() {
      p === void 0 && (p = 1);
      for (var m = h.getCursor(), v = h.getSearchCursor(d, m), x = 0; x < p; x++) {
        var w = v.find(c);
        if (x == 0 && w && xt(v.from(), m)) {
          var S = c ? v.from() : v.to();
          w = v.find(c), w && !w[0] && xt(v.from(), S) && h.getLine(S.line).length == S.ch && (w = v.find(c));
        }
        if (!w && (v = h.getSearchCursor(
          d,
          // @ts-ignore
          c ? new e(h.lastLine()) : new e(h.firstLine(), 0)
        ), !v.find(c)))
          return;
      }
      return v.from();
    });
  }
  function mg(h, c, d, p, m) {
    return h.operation(function() {
      p === void 0 && (p = 1);
      var v = h.getCursor(), x = h.getSearchCursor(d, v), w = x.find(!c);
      !m.visualMode && w && xt(x.from(), v) && x.find(!c);
      for (var S = 0; S < p; S++)
        if (w = x.find(c), !w && (x = h.getSearchCursor(
          d,
          // @ts-ignore
          c ? new e(h.lastLine()) : new e(h.firstLine(), 0)
        ), !x.find(c)))
          return;
      var A = x.from(), O = x.to();
      return A && O && [A, O];
    });
  }
  function en(h) {
    var c = Bt(h);
    c.highlightTimeout && (clearTimeout(c.highlightTimeout), c.highlightTimeout = void 0), h.removeOverlay(Bt(h).getOverlay()), c.setOverlay(null), c.getScrollbarAnnotate() && (c.getScrollbarAnnotate().clear(), c.setScrollbarAnnotate(null));
  }
  function vg(h, c, d) {
    return typeof h != "number" && (h = h.line), c instanceof Array ? le(h, c) : typeof d == "number" ? h >= c && h <= d : h == c;
  }
  function io(h) {
    var c = h.getScrollInfo(), d = 6, p = 10, m = h.coordsChar({ left: 0, top: d + c.top }, "local"), v = c.clientHeight - p + c.top, x = h.coordsChar({ left: 0, top: v }, "local");
    return { top: m.line, bottom: x.line };
  }
  function xr(h, c, d) {
    if (d == "'" || d == "`")
      return q.jumpList.find(h, -1) || new e(0, 0);
    if (d == ".")
      return th(h);
    var p = c.marks[d];
    return p && p.find();
  }
  function th(h) {
    if (h.getLastEditEnd)
      return h.getLastEditEnd();
    for (var c = (
      /**@type{any}*/
      h.doc.history.done
    ), d = c.length; d--; )
      if (c[d].changes)
        return Se(c[d].changes[0].to);
  }
  class yg {
    constructor() {
      this.commandMap_, this.buildCommandMap_();
    }
    /**
     * @arg {CodeMirrorV} cm
     * @arg {string} input
     * @arg {{ callback: () => void; } | undefined} [opt_params]
     */
    processCommand(c, d, p) {
      var m = this;
      c.operation(function() {
        c.curOp && (c.curOp.isVimOp = !0), m._processCommand(c, d, p);
      });
    }
    /**
     * @arg {CodeMirrorV} cm
     * @arg {string} input
     * @arg {{ callback?: () => void; input?: string, line?: string, commandName?: string  } } [opt_params]
     */
    _processCommand(c, d, p) {
      var m = c.state.vim, v = q.registerController.getRegister(":"), x = v.toString(), w = new n.StringStream(d);
      v.setText(d);
      var S = p || {};
      S.input = d;
      try {
        this.parseInput_(c, w, S);
      } catch (L) {
        throw de(c, L + ""), L;
      }
      m.visualMode && zt(c);
      var A, O;
      if (!S.commandName)
        S.line !== void 0 && (O = "move");
      else if (A = this.matchCommand_(S.commandName), A) {
        if (O = A.name, A.excludeFromCommandHistory && v.setText(x), this.parseCommandArgs_(w, S, A), A.type == "exToKey") {
          fi(c, A.toKeys || "", A);
          return;
        } else if (A.type == "exToEx") {
          this.processCommand(c, A.toInput || "");
          return;
        }
      }
      if (!O) {
        de(c, 'Not an editor command ":' + d + '"');
        return;
      }
      try {
        ih[O](c, S), (!A || !A.possiblyAsync) && S.callback && S.callback();
      } catch (L) {
        throw de(c, L + ""), L;
      }
    }
    /**
     * @param {CodeMirrorV} cm
     * @param {import("@codemirror/language").StringStream} inputStream
     * @param {{ callback?: (() => void) | undefined; input?: string | undefined; line?: any; commandName?: any; lineEnd?: any; selectionLine?: any; selectionLineEnd?: any; }} result
     */
    parseInput_(c, d, p) {
      d.eatWhile(":"), d.eat("%") ? (p.line = c.firstLine(), p.lineEnd = c.lastLine()) : (p.line = this.parseLineSpec_(c, d), p.line !== void 0 && d.eat(",") && (p.lineEnd = this.parseLineSpec_(c, d))), p.line == null ? c.state.vim.visualMode ? (p.selectionLine = xr(c, c.state.vim, "<")?.line, p.selectionLineEnd = xr(c, c.state.vim, ">")?.line) : p.selectionLine = c.getCursor().line : (p.selectionLine = p.line, p.selectionLineEnd = p.lineEnd);
      var m = d.match(/^(\w+|!!|@@|[!#&*<=>@~])/);
      return m ? p.commandName = m[1] : p.commandName = (d.match(/.*/) || [""])[0], p;
    }
    /**
     * @param {CodeMirrorV} cm
     * @param {import("@codemirror/language").StringStream} inputStream
     */
    parseLineSpec_(c, d) {
      var p = d.match(/^(\d+)/);
      if (p)
        return parseInt(p[1], 10) - 1;
      switch (d.next()) {
        case ".":
          return this.parseLineSpecOffset_(d, c.getCursor().line);
        case "$":
          return this.parseLineSpecOffset_(d, c.lastLine());
        case "'":
          var m = d.next() || "", v = xr(c, c.state.vim, m);
          if (!v) throw new Error("Mark not set");
          return this.parseLineSpecOffset_(d, v.line);
        case "-":
        case "+":
          return d.backUp(1), this.parseLineSpecOffset_(d, c.getCursor().line);
        default:
          d.backUp(1);
          return;
      }
    }
    /**
     * @param {string | import("@codemirror/language").StringStream} inputStream
     * @param {number} line
     */
    parseLineSpecOffset_(c, d) {
      var p = c.match(/^([+-])?(\d+)/);
      if (p) {
        var m = parseInt(p[2], 10);
        p[1] == "-" ? d -= m : d += m;
      }
      return d;
    }
    /**
     * @param {import("@codemirror/language").StringStream} inputStream
     * @param {import("./types").exCommandArgs} params
     * @param {import("./types").exCommandDefinition} command
     */
    parseCommandArgs_(c, d, p) {
      if (!c.eol()) {
        d.argString = c.match(/.*/)?.[0];
        var m = p.argDelimiter || /\s+/, v = Gs(d.argString || "").split(m);
        v.length && v[0] && (d.args = v);
      }
    }
    /**
     * @arg {string} commandName
     */
    matchCommand_(c) {
      for (var d = c.length; d > 0; d--) {
        var p = c.substring(0, d);
        if (this.commandMap_[p]) {
          var m = this.commandMap_[p];
          if (m.name.indexOf(c) === 0)
            return m;
        }
      }
    }
    buildCommandMap_() {
      this.commandMap_ = {};
      for (var c = 0; c < o.length; c++) {
        var d = o[c], p = d.shortName || d.name;
        this.commandMap_[p] = d;
      }
    }
    /**@type {(lhs: string, rhs: string, ctx: string|void, noremap?: boolean) => void} */
    map(c, d, p, m) {
      if (c != ":" && c.charAt(0) == ":") {
        if (p)
          throw Error("Mode not supported for ex mappings");
        var v = c.substring(1);
        d != ":" && d.charAt(0) == ":" ? this.commandMap_[v] = {
          name: v,
          type: "exToEx",
          toInput: d.substring(1),
          user: !0
        } : this.commandMap_[v] = {
          name: v,
          type: "exToKey",
          toKeys: d,
          user: !0
        };
      } else {
        var x = {
          keys: c,
          type: "keyToKey",
          toKeys: d,
          noremap: !!m
        };
        p && (x.context = p), no(x);
      }
    }
    /**@type {(lhs: string, ctx: string) => boolean|void} */
    unmap(c, d) {
      if (c != ":" && c.charAt(0) == ":") {
        if (d)
          throw Error("Mode not supported for ex mappings");
        var p = c.substring(1);
        if (this.commandMap_[p] && this.commandMap_[p].user)
          return delete this.commandMap_[p], !0;
      } else
        for (var m = c, v = 0; v < i.length; v++)
          if (m == i[v].keys && i[v].context === d)
            return i.splice(v, 1), kg(m), !0;
    }
  }
  var ih = {
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    colorscheme: function(h, c) {
      if (!c.args || c.args.length < 1) {
        de(h, h.getOption("theme"));
        return;
      }
      h.setOption("theme", c.args[0]);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params @arg {'insert'|'normal'|string} [ctx] @arg {boolean} [defaultOnly]*/
    map: function(h, c, d, p) {
      var m = c.args;
      if (!m || m.length < 2) {
        h && de(h, "Invalid mapping: " + c.input);
        return;
      }
      lt.map(m[0], m[1], d, p);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    imap: function(h, c) {
      this.map(h, c, "insert");
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    nmap: function(h, c) {
      this.map(h, c, "normal");
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    vmap: function(h, c) {
      this.map(h, c, "visual");
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    omap: function(h, c) {
      this.map(h, c, "operatorPending");
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    noremap: function(h, c) {
      this.map(h, c, void 0, !0);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    inoremap: function(h, c) {
      this.map(h, c, "insert", !0);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    nnoremap: function(h, c) {
      this.map(h, c, "normal", !0);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    vnoremap: function(h, c) {
      this.map(h, c, "visual", !0);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    onoremap: function(h, c) {
      this.map(h, c, "operatorPending", !0);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params @arg {string} ctx*/
    unmap: function(h, c, d) {
      var p = c.args;
      (!p || p.length < 1 || !lt.unmap(p[0], d)) && h && de(h, "No such mapping: " + c.input);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    mapclear: function(h, c) {
      we.mapclear();
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    imapclear: function(h, c) {
      we.mapclear("insert");
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    nmapclear: function(h, c) {
      we.mapclear("normal");
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    vmapclear: function(h, c) {
      we.mapclear("visual");
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    omapclear: function(h, c) {
      we.mapclear("operatorPending");
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    move: function(h, c) {
      Ri.processCommand(h, h.state.vim, {
        keys: "",
        type: "motion",
        motion: "moveToLineOrEdgeOfDocument",
        motionArgs: { forward: !1, explicitRepeat: !0, linewise: !0 },
        repeatOverride: c.line + 1
      });
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    set: function(h, c) {
      var d = c.args, p = c.setCfg || {};
      if (!d || d.length < 1) {
        h && de(h, "Invalid mapping: " + c.input);
        return;
      }
      var m = d[0].split("="), v = m.shift() || "", x = m.length > 0 ? m.join("=") : void 0, w = !1, S = !1;
      if (v.charAt(v.length - 1) == "?") {
        if (x)
          throw Error("Trailing characters: " + c.argString);
        v = v.substring(0, v.length - 1), w = !0;
      } else v.charAt(v.length - 1) == "!" && (v = v.substring(0, v.length - 1), S = !0);
      x === void 0 && v.substring(0, 2) == "no" && (v = v.substring(2), x = !1);
      var A = he[v] && he[v].type == "boolean";
      if (A && (S ? x = !ie(v, h, p) : x == null && (x = !0)), !A && x === void 0 || w) {
        var O = ie(v, h, p);
        O instanceof Error ? de(h, O.message) : O === !0 || O === !1 ? de(h, " " + (O ? "" : "no") + v) : de(h, "  " + v + "=" + O);
      } else {
        var L = Y(v, x, h, p);
        L instanceof Error && de(h, L.message);
      }
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    setlocal: function(h, c) {
      c.setCfg = { scope: "local" }, this.set(h, c);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    setglobal: function(h, c) {
      c.setCfg = { scope: "global" }, this.set(h, c);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    registers: function(h, c) {
      var d = c.args, p = q.registerController.registers, m = `----------Registers----------

`;
      if (d)
        for (var w = d.join(""), S = 0; S < w.length; S++) {
          var v = w.charAt(S);
          if (q.registerController.isValidRegister(v)) {
            var A = p[v] || new Vt();
            m += '"' + v + "    " + A.toString() + `
`;
          }
        }
      else
        for (var v in p) {
          var x = p[v].toString();
          x.length && (m += '"' + v + "    " + x + `
`);
        }
      de(h, m, !0);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    marks: function(h, c) {
      var d = c.args, p = h.state.vim.marks, m = `-----------Marks-----------
mark	line	col

`;
      if (d)
        for (var w = d.join(""), S = 0; S < w.length; S++) {
          var v = w.charAt(S), x = p[v] && p[v].find();
          x && (m += v + "	" + x.line + "	" + x.ch + `
`);
        }
      else
        for (var v in p) {
          var x = p[v] && p[v].find();
          x && (m += v + "	" + x.line + "	" + x.ch + `
`);
        }
      de(h, m, !0);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    sort: function(h, c) {
      var d, p, m, v, x;
      function w() {
        if (c.argString) {
          var ue = new n.StringStream(c.argString);
          if (ue.eat("!") && (d = !0), ue.eol())
            return;
          if (!ue.eatSpace())
            return "Invalid arguments";
          var J = ue.match(/([dinuox]+)?\s*(\/.+\/)?\s*/);
          if (!J || !ue.eol())
            return "Invalid arguments";
          if (J[1]) {
            p = J[1].indexOf("i") != -1, m = J[1].indexOf("u") != -1;
            var Ce = J[1].indexOf("d") != -1 || J[1].indexOf("n") != -1, kt = J[1].indexOf("x") != -1, _e = J[1].indexOf("o") != -1;
            if (Number(Ce) + Number(kt) + Number(_e) > 1)
              return "Invalid arguments";
            v = Ce && "decimal" || kt && "hex" || _e && "octal";
          }
          J[2] && (x = new RegExp(J[2].substr(1, J[2].length - 2), p ? "i" : ""));
        }
      }
      var S = w();
      if (S) {
        de(h, S + ": " + c.argString);
        return;
      }
      var A = c.line || h.firstLine(), O = c.lineEnd || c.line || h.lastLine();
      if (A == O)
        return;
      var L = new e(A, 0), D = new e(O, Ze(h, O)), B = h.getRange(L, D).split(`
`), T = v == "decimal" ? /(-?)([\d]+)/ : v == "hex" ? /(-?)(?:0x)?([0-9a-f]+)/i : v == "octal" ? /([0-7]+)/ : null, W = v == "decimal" ? 10 : v == "hex" ? 16 : v == "octal" ? 8 : void 0, V = [], X = [];
      if (v || x)
        for (var K = 0; K < B.length; K++) {
          var te = x ? B[K].match(x) : null;
          te && te[0] != "" ? V.push(te) : T && T.exec(B[K]) ? V.push(B[K]) : X.push(B[K]);
        }
      else
        X = B;
      function ae(ue, J) {
        if (d) {
          var Ce;
          Ce = ue, ue = J, J = Ce;
        }
        p && (ue = ue.toLowerCase(), J = J.toLowerCase());
        var kt = T && T.exec(ue), _e = T && T.exec(J);
        if (!kt || !_e)
          return ue < J ? -1 : 1;
        var wt = parseInt((kt[1] + kt[2]).toLowerCase(), W), Li = parseInt((_e[1] + _e[2]).toLowerCase(), W);
        return wt - Li;
      }
      function re(ue, J) {
        if (d) {
          var Ce;
          Ce = ue, ue = J, J = Ce;
        }
        return p && (ue[0] = ue[0].toLowerCase(), J[0] = J[0].toLowerCase()), ue[0] < J[0] ? -1 : 1;
      }
      if (V.sort(x ? re : ae), x)
        for (var K = 0; K < V.length; K++)
          V[K] = V[K].input;
      else v || X.sort(ae);
      if (B = d ? V.concat(X) : X.concat(V), m) {
        var Ae = B, Pe;
        B = [];
        for (var K = 0; K < Ae.length; K++)
          Ae[K] != Pe && B.push(Ae[K]), Pe = Ae[K];
      }
      h.replaceRange(B.join(`
`), L, D);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    vglobal: function(h, c) {
      this.global(h, c);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    normal: function(h, c) {
      var d = !1, p = c.argString;
      if (p && p[0] == "!" && (p = p.slice(1), d = !0), p = p.trimStart(), !p) {
        de(h, "Argument is required.");
        return;
      }
      var m = c.line;
      if (typeof m == "number")
        for (var v = isNaN(c.lineEnd) ? m : c.lineEnd, x = m; x <= v; x++)
          h.setCursor(x, 0), fi(h, c.argString.trimStart(), { noremap: d }), h.state.vim.insertMode && di(h, !0);
      else
        fi(h, c.argString.trimStart(), { noremap: d }), h.state.vim.insertMode && di(h, !0);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    global: function(h, c) {
      var d = c.argString;
      if (!d) {
        de(h, "Regular Expression missing from global");
        return;
      }
      var p = c.commandName[0] === "v";
      d[0] === "!" && c.commandName[0] === "g" && (p = !0, d = d.slice(1));
      var m = c.line !== void 0 ? c.line : h.firstLine(), v = c.lineEnd || c.line || h.lastLine(), x = ag(d), w = d, S = "";
      if (x && x.length && (w = x[0], S = x.slice(1, x.length).join("/")), w)
        try {
          Ln(
            h,
            w,
            !0,
            !0
            /** smartCase */
          );
        } catch {
          de(h, "Invalid regex: " + w);
          return;
        }
      for (var A = Bt(h).getQuery(), O = [], L = m; L <= v; L++) {
        var D = h.getLine(L), B = A.test(D);
        B !== p && O.push(S ? h.getLineHandle(L) : D);
      }
      if (!S) {
        de(h, O.join(`
`));
        return;
      }
      var T = 0, W = function() {
        if (T < O.length) {
          var V = O[T++], X = h.getLineNumber(V);
          if (X == null) {
            W();
            return;
          }
          var K = X + 1 + S;
          lt.processCommand(h, K, {
            callback: W
          });
        } else h.releaseLineHandles && h.releaseLineHandles();
      };
      W();
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    substitute: function(h, c) {
      if (!h.getSearchCursor)
        throw new Error("Search feature not available. Requires searchcursor.js or any other getSearchCursor implementation.");
      var d = c.argString, p = d ? Xa(d, d[0]) : [], m = "", v = "", x, w, S, A = !1, O = !1;
      if (p && p.length)
        m = p[0], ie("pcre") && m !== "" && (m = new RegExp(m).source), v = p[1], v !== void 0 && (ie("pcre") ? v = ug(v.replace(/([^\\])&/g, "$1$$&")) : v = fg(v), q.lastSubstituteReplacePart = v), x = p[2] ? p[2].split(" ") : [];
      else if (d && d.length) {
        de(h, "Substitutions should be of the form :s/pattern/replace/");
        return;
      }
      if (x && (w = x[0], S = parseInt(x[1]), w && (w.indexOf("c") != -1 && (A = !0), w.indexOf("g") != -1 && (O = !0), ie("pcre") ? m = m + "/" + w : m = m.replace(/\//g, "\\/") + "/" + w)), m)
        try {
          Ln(
            h,
            m,
            !0,
            !0
            /** smartCase */
          );
        } catch {
          de(h, "Invalid regex: " + m);
          return;
        }
      if (v = v || q.lastSubstituteReplacePart, v === void 0) {
        de(h, "No previous substitute regular expression");
        return;
      }
      var L = Bt(h), D = L.getQuery(), B = c.line !== void 0 ? c.line : h.getCursor().line, T = c.lineEnd || B;
      B == h.firstLine() && T == h.lastLine() && (T = 1 / 0), S && (B = T, T = B + S - 1);
      var W = it(h, new e(B, 0)), V = h.getSearchCursor(D, W);
      bg(h, A, O, B, T, V, D, v, c.callback);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    startinsert: function(h, c) {
      fi(h, c.argString == "!" ? "A" : "i", {});
    },
    redo: n.commands.redo,
    undo: n.commands.undo,
    /** @arg {CodeMirrorV} cm */
    write: function(h) {
      n.commands.save ? n.commands.save(h) : h.save && h.save();
    },
    /** @arg {CodeMirrorV} cm */
    nohlsearch: function(h) {
      en(h);
    },
    /** @arg {CodeMirrorV} cm */
    yank: function(h) {
      var c = Se(h.getCursor()), d = c.line, p = h.getLine(d);
      q.registerController.pushText(
        "0",
        "yank",
        p,
        !0,
        !0
      );
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    delete: function(h, c) {
      var d = c.selectionLine, p = isNaN(c.selectionLineEnd) ? d : c.selectionLineEnd;
      Ys.delete(h, { linewise: !0 }, [
        {
          anchor: new e(d, 0),
          head: new e(p + 1, 0)
        }
      ]);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    join: function(h, c) {
      var d = c.selectionLine, p = isNaN(c.selectionLineEnd) ? d : c.selectionLineEnd;
      h.setCursor(new e(d, 0)), Zi.joinLines(h, { repeat: p - d }, h.state.vim);
    },
    /** @arg {CodeMirrorV} cm @arg {ExParams} params*/
    delmarks: function(h, c) {
      if (!c.argString || !Gs(c.argString)) {
        de(h, "Argument required");
        return;
      }
      for (var d = h.state.vim, p = new n.StringStream(Gs(c.argString)); !p.eol(); ) {
        p.eatSpace();
        var m = p.pos;
        if (!p.match(/[a-zA-Z]/, !1)) {
          de(h, "Invalid argument: " + c.argString.substring(m));
          return;
        }
        var v = p.next();
        if (p.match("-", !0)) {
          if (!p.match(/[a-zA-Z]/, !1)) {
            de(h, "Invalid argument: " + c.argString.substring(m));
            return;
          }
          var x = v, w = p.next();
          if (x && w && N(x) == N(w)) {
            var S = x.charCodeAt(0), A = w.charCodeAt(0);
            if (S >= A) {
              de(h, "Invalid argument: " + c.argString.substring(m));
              return;
            }
            for (var O = 0; O <= A - S; O++) {
              var L = String.fromCharCode(S + O);
              delete d.marks[L];
            }
          } else {
            de(h, "Invalid argument: " + x + "-");
            return;
          }
        } else v && delete d.marks[v];
      }
    }
  }, lt = new yg();
  we.defineEx("version", "ve", (h) => {
    de(h, "Codemirror-vim version: 6.3.0");
  });
  function bg(h, c, d, p, m, v, x, w, S) {
    h.state.vim.exMode = !0;
    var A = !1, O = 0, L, D, B;
    function T() {
      h.operation(function() {
        for (; !A; )
          W(), X();
        K();
      });
    }
    function W() {
      var ae = "", re = v.match || v.pos && v.pos.match;
      if (re)
        ae = w.replace(/\$(\d{1,3}|[$&])/g, function(ue, J) {
          if (J == "$") return "$";
          if (J == "&") return re[0];
          for (var Ce = J; parseInt(Ce) >= re.length && Ce.length > 0; )
            Ce = Ce.slice(0, Ce.length - 1);
          return Ce ? re[Ce] + J.slice(Ce.length, J.length) : ue;
        });
      else {
        var Ae = h.getRange(v.from(), v.to());
        ae = Ae.replace(x, w);
      }
      var Pe = v.to().line;
      v.replace(ae), D = v.to().line, m += D - Pe, B = D < Pe;
    }
    function V() {
      var ae = L && Se(v.to()), re = v.findNext();
      return re && !re[0] && ae && xt(v.from(), ae) && (re = v.findNext()), re && O++, re;
    }
    function X() {
      for (; V() && vg(v.from(), p, m); )
        if (!(!d && v.from().line == D && !B)) {
          h.scrollIntoView(v.from(), 30), h.setSelection(v.from(), v.to()), L = v.from(), A = !1;
          return;
        }
      A = !0;
    }
    function K(ae) {
      if (ae && ae(), h.focus(), L) {
        h.setCursor(L);
        var re = h.state.vim;
        re.exMode = !1, re.lastHPos = re.lastHSPos = L.ch;
      }
      S ? S() : A && de(
        h,
        (O ? "Found " + O + " matches" : "No matches found") + " for pattern: " + x + (ie("pcre") ? " (set nopcre to use Vim regexps)" : "")
      );
    }
    function te(ae, re, Ae) {
      n.e_stop(ae);
      var Pe = Rn(ae);
      switch (Pe) {
        case "y":
          W(), X();
          break;
        case "n":
          X();
          break;
        case "a":
          var ue = S;
          S = void 0, h.operation(T), S = ue;
          break;
        case "l":
          W();
        // fall through and exit.
        case "q":
        case "<Esc>":
        case "<C-c>":
        case "<C-[>":
          K(Ae);
          break;
      }
      return A && K(Ae), !0;
    }
    if (X(), A) {
      de(h, "No matches for " + x + (ie("pcre") ? " (set nopcre to use vim regexps)" : ""));
      return;
    }
    if (!c) {
      T(), S && S();
      return;
    }
    br(h, {
      prefix: Et("span", "replace with ", Et("strong", w), " (y/n/a/q/l)"),
      onKeyDown: te
    });
  }
  function di(h, c) {
    var d = h.state.vim, p = q.macroModeState, m = q.registerController.getRegister("."), v = p.isPlaying, x = p.lastInsertModeChanges;
    v || (h.off("change", nh), d.insertEnd && d.insertEnd.clear(), d.insertEnd = void 0, n.off(h.getInputField(), "keydown", oh)), !v && d.insertModeRepeat && d.insertModeRepeat > 1 && (lh(
      h,
      d,
      d.insertModeRepeat - 1,
      !0
      /** repeatForInsert */
    ), d.lastEditInputState.repeatOverride = d.insertModeRepeat), delete d.insertModeRepeat, d.insertMode = !1, c || h.setCursor(h.getCursor().line, h.getCursor().ch - 1), h.setOption("keyMap", "vim"), h.setOption("disableInput", !0), h.toggleOverwrite(!1), m.setText(x.changes.join("")), n.signal(h, "vim-mode-change", { mode: "normal" }), p.isRecording && Og(p);
  }
  function no(h) {
    i.unshift(h), h.keys && xg(h.keys);
  }
  function xg(h) {
    h.split(/(<(?:[CSMA]-)*\w+>|.)/i).forEach(function(c) {
      c && (r[c] || (r[c] = 0), r[c]++);
    });
  }
  function kg(h) {
    h.split(/(<(?:[CSMA]-)*\w+>|.)/i).forEach(function(c) {
      r[c] && r[c]--;
    });
  }
  function wg(h, c, d, p, m) {
    var v = { keys: h, type: c };
    v[c] = d, v[c + "Args"] = p;
    for (var x in m)
      v[x] = m[x];
    no(v);
  }
  ee("insertModeEscKeysTimeout", 200, "number");
  function Sg(h, c, d, p) {
    var m = q.registerController.getRegister(p);
    if (p == ":") {
      m.keyBuffer[0] && lt.processCommand(h, m.keyBuffer[0]), d.isPlaying = !1;
      return;
    }
    var v = m.keyBuffer, x = 0;
    d.isPlaying = !0, d.replaySearchQueries = m.searchQueries.slice(0);
    for (var w = 0; w < v.length; w++)
      for (var S = v[w], A, O, L = /<(?:[CSMA]-)*\w+>|./gi; A = L.exec(S); )
        if (O = A[0], we.handleKey(h, O, "macro"), c.insertMode) {
          var D = m.insertModeChanges[x++].changes;
          q.macroModeState.lastInsertModeChanges.changes = D, hh(h, D, 1), di(h);
        }
    d.isPlaying = !1;
  }
  function Cg(h, c) {
    if (!h.isPlaying) {
      var d = h.latestRegister, p = q.registerController.getRegister(d);
      p && p.pushText(c);
    }
  }
  function Og(h) {
    if (!h.isPlaying) {
      var c = h.latestRegister, d = q.registerController.getRegister(c);
      d && d.pushInsertModeChanges && d.pushInsertModeChanges(h.lastInsertModeChanges);
    }
  }
  function Ag(h, c) {
    if (!h.isPlaying) {
      var d = h.latestRegister, p = q.registerController.getRegister(d);
      p && p.pushSearchQuery && p.pushSearchQuery(c);
    }
  }
  function nh(h, c) {
    var d = q.macroModeState, p = d.lastInsertModeChanges;
    if (!d.isPlaying)
      for (var m = h.state.vim; c; ) {
        if (p.expectCursorActivityForChange = !0, p.ignoreCount > 1)
          p.ignoreCount--;
        else if (c.origin == "+input" || c.origin == "paste" || c.origin === void 0) {
          var v = h.listSelections().length;
          v > 1 && (p.ignoreCount = v);
          var x = c.text.join(`
`);
          if (p.maybeReset && (p.changes = [], p.maybeReset = !1), x)
            if (h.state.overwrite && !/\n/.test(x))
              p.changes.push([x]);
            else {
              if (x.length > 1) {
                var w = m && m.insertEnd && m.insertEnd.find(), S = h.getCursor();
                if (w && w.line == S.line) {
                  var A = w.ch - S.ch;
                  A > 0 && A < x.length && (p.changes.push([x, A]), x = "");
                }
              }
              x && p.changes.push(x);
            }
        }
        c = c.next;
      }
  }
  function rh(h) {
    var c = h.state.vim;
    if (c.insertMode) {
      var d = q.macroModeState;
      if (d.isPlaying)
        return;
      var p = d.lastInsertModeChanges;
      p.expectCursorActivityForChange ? p.expectCursorActivityForChange = !1 : (p.maybeReset = !0, c.insertEnd && c.insertEnd.clear(), c.insertEnd = h.setBookmark(h.getCursor(), { insertLeft: !0 }));
    } else h.curOp?.isVimOp || sh(h, c);
  }
  function sh(h, c) {
    var d = h.getCursor("anchor"), p = h.getCursor("head");
    if (c.visualMode && !h.somethingSelected() ? zt(h, !1) : !c.visualMode && !c.insertMode && h.somethingSelected() && (c.visualMode = !0, c.visualLine = !1, n.signal(h, "vim-mode-change", { mode: "visual" })), c.visualMode) {
      var m = Re(p, d) ? 0 : -1, v = Re(p, d) ? -1 : 0;
      p = Ie(p, 0, m), d = Ie(d, 0, v), c.sel = {
        anchor: d,
        head: p
      }, ii(h, c, "<", nt(p, d)), ii(h, c, ">", ui(p, d));
    } else c.insertMode || (c.lastHPos = h.getCursor().ch);
  }
  function oh(h) {
    var c = q.macroModeState, d = c.lastInsertModeChanges, p = n.keyName ? n.keyName(h) : h.key;
    p && (p.indexOf("Delete") != -1 || p.indexOf("Backspace") != -1) && (d.maybeReset && (d.changes = [], d.maybeReset = !1), d.changes.push(new Ge(p, h)));
  }
  function lh(h, c, d, p) {
    var m = q.macroModeState;
    m.isPlaying = !0;
    var v = c.lastEditActionCommand, x = c.inputState;
    function w() {
      v ? Ri.processAction(h, c, v) : Ri.evalInput(h, c);
    }
    function S(O) {
      if (m.lastInsertModeChanges.changes.length > 0) {
        O = c.lastEditActionCommand ? O : 1;
        var L = m.lastInsertModeChanges;
        hh(h, L.changes, O);
      }
    }
    if (c.inputState = c.lastEditInputState, v && v.interlaceInsertRepeat)
      for (var A = 0; A < d; A++)
        w(), S(1);
    else
      p || w(), S(d);
    c.inputState = x, c.insertMode && !p && di(h), m.isPlaying = !1;
  }
  function ah(h, c) {
    n.lookupKey(c, "vim-insert", function(p) {
      return typeof p == "string" ? n.commands[p](h) : p(h), !0;
    });
  }
  function hh(h, c, d) {
    var p = h.getCursor("head"), m = q.macroModeState.lastInsertModeChanges.visualBlock;
    m && (Qa(h, p, m + 1), d = h.listSelections().length, h.setCursor(p));
    for (var v = 0; v < d; v++) {
      m && h.setCursor(Ie(p, v, 0));
      for (var x = 0; x < c.length; x++) {
        var w = c[x];
        if (w instanceof Ge)
          ah(h, w.keyName);
        else if (typeof w == "string")
          h.replaceSelection(w);
        else {
          var S = h.getCursor(), A = Ie(S, 0, w[0].length - (w[1] || 0));
          h.replaceRange(w[0], S, w[1] ? S : A), h.setCursor(A);
        }
      }
    }
    m && h.setCursor(Ie(p, 0, 1));
  }
  function ro(h) {
    var c = new h.constructor();
    return Object.keys(h).forEach(function(d) {
      if (d != "insertEnd") {
        var p = h[d];
        Array.isArray(p) ? p = p.slice() : p && typeof p == "object" && p.constructor != Object && (p = ro(p)), c[d] = p;
      }
    }), h.sel && (c.sel = {
      head: h.sel.head && Se(h.sel.head),
      anchor: h.sel.anchor && Se(h.sel.anchor)
    }), c;
  }
  function Mg(h, c, d) {
    var v = Be(h), p = (
      /**@type {CodeMirrorV}*/
      h
    ), m = !1, v = we.maybeInitVimState_(p), x = v.visualBlock || v.wasInVisualBlock;
    if (p.state.closeVimNotification) {
      var w = p.state.closeVimNotification;
      if (p.state.closeVimNotification = null, w(), c == "<CR>")
        return He(p), !0;
    }
    var S = p.isInMultiSelectMode();
    if (v.wasInVisualBlock && !S ? v.wasInVisualBlock = !1 : S && v.visualBlock && (v.wasInVisualBlock = !0), c == "<Esc>" && !v.insertMode && !v.visualMode && S && v.status == "<Esc>")
      He(p);
    else if (x || !S || p.inVirtualSelectionMode)
      m = we.handleKey(p, c, d);
    else {
      var A = ro(v), O = v.inputState.changeQueueList || [];
      p.operation(function() {
        p.curOp && (p.curOp.isVimOp = !0);
        var L = 0;
        p.forEachSelection(function() {
          p.state.vim.inputState.changeQueue = O[L];
          var D = p.getCursor("head"), B = p.getCursor("anchor"), T = Re(D, B) ? 0 : -1, W = Re(D, B) ? -1 : 0;
          D = Ie(D, 0, T), B = Ie(B, 0, W), p.state.vim.sel.head = D, p.state.vim.sel.anchor = B, m = we.handleKey(p, c, d), p.virtualSelection && (O[L] = p.state.vim.inputState.changeQueue, p.state.vim = ro(A)), L++;
        }), p.curOp?.cursorActivity && !m && (p.curOp.cursorActivity = !1), p.state.vim = v, v.inputState.changeQueueList = O, v.inputState.changeQueue = null;
      }, !0);
    }
    return m && !v.visualMode && !v.insertMode && v.visualMode != p.somethingSelected() && sh(p, v), m;
  }
  return Ee(), we;
}
function mt(n, e) {
  var t = e.ch, i = e.line + 1;
  i < 1 && (i = 1, t = 0), i > n.lines && (i = n.lines, t = Number.MAX_VALUE);
  var r = n.line(i);
  return Math.min(r.from + Math.max(0, t), r.to);
}
function St(n, e) {
  let t = n.lineAt(e);
  return { line: t.number - 1, ch: e - t.from };
}
class Ut {
  constructor(e, t) {
    this.line = e, this.ch = t;
  }
}
function Cp(n, e, t) {
  if (n.addEventListener)
    n.addEventListener(e, t, !1);
  else {
    var i = n._handlers || (n._handlers = {});
    i[e] = (i[e] || []).concat(t);
  }
}
function Op(n, e, t) {
  if (n.removeEventListener)
    n.removeEventListener(e, t, !1);
  else {
    var i = n._handlers, r = i && i[e];
    if (r) {
      var s = r.indexOf(t);
      s > -1 && (i[e] = r.slice(0, s).concat(r.slice(s + 1)));
    }
  }
}
function Ap(n, e, ...t) {
  var i, r = (i = n._handlers) === null || i === void 0 ? void 0 : i[e];
  if (r)
    for (var s = 0; s < r.length; ++s)
      r[s](...t);
}
function yf(n, ...e) {
  if (n)
    for (var t = 0; t < n.length; ++t)
      n[t](...e);
}
let Fl;
try {
  Fl = /* @__PURE__ */ new RegExp("[\\w\\p{Alphabetic}\\p{Number}_]", "u");
} catch {
  Fl = /[\w]/;
}
function zn(n, e) {
  var t = n.cm6;
  if (!t.state.readOnly) {
    var i = "input.type.compose";
    if (n.curOp && (n.curOp.lastChange || (i = "input.type.compose.start")), e.annotations)
      try {
        e.annotations.some(function(r) {
          r.value == "input" && (r.value = i);
        });
      } catch (r) {
        console.error(r);
      }
    else
      e.userEvent = i;
    return t.dispatch(e);
  }
}
function bf(n, e) {
  var t;
  n.curOp && (n.curOp.$changeStart = void 0), (e ? pa : bs)(n.cm6);
  let i = (t = n.curOp) === null || t === void 0 ? void 0 : t.$changeStart;
  i != null && n.cm6.dispatch({ selection: { anchor: i } });
}
var qk = {
  Left: (n) => Ni(n.cm6, { key: "Left" }, "editor"),
  Right: (n) => Ni(n.cm6, { key: "Right" }, "editor"),
  Up: (n) => Ni(n.cm6, { key: "Up" }, "editor"),
  Down: (n) => Ni(n.cm6, { key: "Down" }, "editor"),
  Backspace: (n) => Ni(n.cm6, { key: "Backspace" }, "editor"),
  Delete: (n) => Ni(n.cm6, { key: "Delete" }, "editor")
};
class se {
  // --------------------------
  openDialog(e, t, i) {
    return _k(this, e, t, i);
  }
  openNotification(e, t) {
    return Kk(this, e, t);
  }
  constructor(e) {
    this.state = {}, this.marks = /* @__PURE__ */ Object.create(null), this.$mid = 0, this.options = {}, this._handlers = {}, this.$lastChangeEndOffset = 0, this.virtualSelection = null, this.cm6 = e, this.onChange = this.onChange.bind(this), this.onSelectionChange = this.onSelectionChange.bind(this);
  }
  on(e, t) {
    Cp(this, e, t);
  }
  off(e, t) {
    Op(this, e, t);
  }
  signal(e, t, i) {
    Ap(this, e, t, i);
  }
  indexFromPos(e) {
    return mt(this.cm6.state.doc, e);
  }
  posFromIndex(e) {
    return St(this.cm6.state.doc, e);
  }
  foldCode(e) {
    let t = this.cm6, i = t.state.selection.ranges, r = this.cm6.state.doc, s = mt(r, e), o = E.create([E.range(s, s)], 0).ranges;
    t.state.selection.ranges = o, ld(t), t.state.selection.ranges = i;
  }
  firstLine() {
    return 0;
  }
  lastLine() {
    return this.cm6.state.doc.lines - 1;
  }
  lineCount() {
    return this.cm6.state.doc.lines;
  }
  setCursor(e, t) {
    typeof e == "object" && (t = e.ch, e = e.line);
    var i = mt(this.cm6.state.doc, { line: e, ch: t || 0 });
    this.cm6.dispatch({ selection: { anchor: i } }, { scrollIntoView: !this.curOp }), this.curOp && !this.curOp.isVimOp && this.onBeforeEndOperation();
  }
  getCursor(e) {
    var t = this.cm6.state.selection.main, i = e == "head" || !e ? t.head : e == "anchor" ? t.anchor : e == "start" ? t.from : e == "end" ? t.to : null;
    if (i == null)
      throw new Error("Invalid cursor type");
    return this.posFromIndex(i);
  }
  listSelections() {
    var e = this.cm6.state.doc;
    return this.cm6.state.selection.ranges.map((t) => ({
      anchor: St(e, t.anchor),
      head: St(e, t.head)
    }));
  }
  setSelections(e, t) {
    var i = this.cm6.state.doc, r = e.map((s) => {
      var o = mt(i, s.head), l = mt(i, s.anchor);
      return o == l ? E.cursor(o, 1) : E.range(l, o);
    });
    this.cm6.dispatch({
      selection: E.create(r, t)
    });
  }
  setSelection(e, t, i) {
    this.setSelections([{ anchor: e, head: t }], 0), i && i.origin == "*mouse" && this.onBeforeEndOperation();
  }
  getLine(e) {
    var t = this.cm6.state.doc;
    return e < 0 || e >= t.lines ? "" : this.cm6.state.doc.line(e + 1).text;
  }
  getLineHandle(e) {
    return this.$lineHandleChanges || (this.$lineHandleChanges = []), { row: e, index: this.indexFromPos(new Ut(e, 0)) };
  }
  getLineNumber(e) {
    var t = this.$lineHandleChanges;
    if (!t)
      return null;
    for (var i = e.index, r = 0; r < t.length; r++)
      if (i = t[r].changes.mapPos(i, 1, Xe.TrackAfter), i == null)
        return null;
    var s = this.posFromIndex(i);
    return s.ch == 0 ? s.line : null;
  }
  releaseLineHandles() {
    this.$lineHandleChanges = void 0;
  }
  getRange(e, t) {
    var i = this.cm6.state.doc;
    return this.cm6.state.sliceDoc(mt(i, e), mt(i, t));
  }
  replaceRange(e, t, i, r) {
    i || (i = t);
    var s = this.cm6.state.doc, o = mt(s, t), l = mt(s, i);
    zn(this, { changes: { from: o, to: l, insert: e } });
  }
  replaceSelection(e) {
    zn(this, this.cm6.state.replaceSelection(e));
  }
  replaceSelections(e) {
    var t = this.cm6.state.selection.ranges, i = t.map((r, s) => ({ from: r.from, to: r.to, insert: e[s] || "" }));
    zn(this, { changes: i });
  }
  getSelection() {
    return this.getSelections().join(`
`);
  }
  getSelections() {
    var e = this.cm6;
    return e.state.selection.ranges.map((t) => e.state.sliceDoc(t.from, t.to));
  }
  somethingSelected() {
    return this.cm6.state.selection.ranges.some((e) => !e.empty);
  }
  getInputField() {
    return this.cm6.contentDOM;
  }
  clipPos(e) {
    var t = this.cm6.state.doc, i = e.ch, r = e.line + 1;
    r < 1 && (r = 1, i = 0), r > t.lines && (r = t.lines, i = Number.MAX_VALUE);
    var s = t.line(r);
    return i = Math.min(Math.max(0, i), s.to - s.from), new Ut(r - 1, i);
  }
  getValue() {
    return this.cm6.state.doc.toString();
  }
  setValue(e) {
    var t = this.cm6;
    return t.dispatch({
      changes: { from: 0, to: t.state.doc.length, insert: e },
      selection: E.range(0, 0)
    });
  }
  focus() {
    return this.cm6.focus();
  }
  blur() {
    return this.cm6.contentDOM.blur();
  }
  defaultTextHeight() {
    return this.cm6.defaultLineHeight;
  }
  findMatchingBracket(e, t) {
    var i = this.cm6.state, r = mt(i.doc, e), s = Tt(i, r + 1, -1);
    return s && s.end ? { to: St(i.doc, s.end.from) } : (s = Tt(i, r, 1), s && s.end ? { to: St(i.doc, s.end.from) } : { to: void 0 });
  }
  scanForBracket(e, t, i, r) {
    return Xk(this, e, t, i, r);
  }
  indentLine(e, t) {
    t ? this.indentMore() : this.indentLess();
  }
  indentMore() {
    Ud(this.cm6);
  }
  indentLess() {
    Xd(this.cm6);
  }
  execCommand(e) {
    if (e == "indentAuto")
      se.commands.indentAuto(this);
    else if (e == "goLineLeft")
      Rd(this.cm6);
    else if (e == "goLineRight") {
      Pd(this.cm6);
      let t = this.cm6.state, i = t.selection.main.head;
      i < t.doc.length && t.sliceDoc(i, i + 1) !== `
` && pb(this.cm6);
    } else
      console.log(e + " is not implemented");
  }
  setBookmark(e, t) {
    var i = t?.insertLeft ? 1 : -1, r = this.indexFromPos(e), s = new Zk(this, r, i);
    return s;
  }
  addOverlay({ query: e }) {
    let t = new xa({
      regexp: !0,
      search: e.source,
      caseSensitive: !/i/.test(e.flags)
    });
    if (t.valid) {
      t.forVim = !0, this.cm6Query = t;
      let i = Mi.of(t);
      return this.cm6.dispatch({ effects: i }), t;
    }
  }
  removeOverlay(e) {
    if (!this.cm6Query)
      return;
    this.cm6Query.forVim = !1;
    let t = Mi.of(this.cm6Query);
    this.cm6.dispatch({ effects: t });
  }
  getSearchCursor(e, t) {
    var i = this, r = null, s = null, o = !1;
    t.ch == null && (t.ch = Number.MAX_VALUE);
    var l = mt(i.cm6.state.doc, t), a = e.source.replace(/(\\.|{(?:\d+(?:,\d*)?|,\d+)})|[{}]/g, function(b, k) {
      return k || "\\" + b;
    });
    function f(b, k = 0, C = b.length) {
      return new ba(b, a, { ignoreCase: e.ignoreCase }, k, C);
    }
    function u(b) {
      var k = i.cm6.state.doc;
      if (b > k.length)
        return null;
      let C = f(k, b).next();
      return C.done ? null : C.value;
    }
    var g = 1e4;
    function y(b, k) {
      var C = i.cm6.state.doc;
      for (let M = 1; ; M++) {
        let R = Math.max(b, k - M * g), I = f(C, R, k), N = null;
        for (; !I.next().done; )
          N = I.value;
        if (N && (R == b || N.from > R + 10))
          return N;
        if (R == b)
          return null;
      }
    }
    return {
      findNext: function() {
        return this.find(!1);
      },
      findPrevious: function() {
        return this.find(!0);
      },
      find: function(b) {
        var k = i.cm6.state.doc;
        if (b) {
          let C = r ? o ? r.to - 1 : r.from : l;
          r = y(0, C);
        } else {
          let C = r ? o ? r.to + 1 : r.to : l;
          r = u(C);
        }
        return s = r && {
          from: St(k, r.from),
          to: St(k, r.to),
          match: r.match
        }, o = r ? r.from == r.to : !1, r && r.match;
      },
      from: function() {
        return s?.from;
      },
      to: function() {
        return s?.to;
      },
      replace: function(b) {
        r && (zn(i, {
          changes: { from: r.from, to: r.to, insert: b }
        }), r.to = r.from + b.length, s && (s.to = St(i.cm6.state.doc, r.to)));
      },
      get match() {
        return s && s.match;
      }
    };
  }
  findPosV(e, t, i, r) {
    let { cm6: s } = this;
    const o = s.state.doc;
    let l = i == "page" ? s.dom.clientHeight : 0;
    const a = mt(o, e);
    let f = E.cursor(a, 1, void 0, r), u = Math.round(Math.abs(t));
    for (let y = 0; y < u; y++)
      i == "page" ? f = s.moveVertically(f, t > 0, l) : i == "line" && (f = s.moveVertically(f, t > 0));
    let g = St(o, f.head);
    return (t < 0 && f.head == 0 && r != 0 && e.line == 0 && e.ch != 0 || t > 0 && f.head == o.length && g.ch != r && e.line == g.line) && (g.hitSide = !0), g;
  }
  charCoords(e, t) {
    var i = this.cm6.contentDOM.getBoundingClientRect(), r = mt(this.cm6.state.doc, e), s = this.cm6.coordsAtPos(r), o = -i.top;
    return { left: (s?.left || 0) - i.left, top: (s?.top || 0) + o, bottom: (s?.bottom || 0) + o };
  }
  coordsChar(e, t) {
    var i = this.cm6.contentDOM.getBoundingClientRect(), r = this.cm6.posAtCoords({ x: e.left + i.left, y: e.top + i.top }) || 0;
    return St(this.cm6.state.doc, r);
  }
  getScrollInfo() {
    var e = this.cm6.scrollDOM;
    return {
      left: e.scrollLeft,
      top: e.scrollTop,
      height: e.scrollHeight,
      width: e.scrollWidth,
      clientHeight: e.clientHeight,
      clientWidth: e.clientWidth
    };
  }
  scrollTo(e, t) {
    e != null && (this.cm6.scrollDOM.scrollLeft = e), t != null && (this.cm6.scrollDOM.scrollTop = t);
  }
  scrollIntoView(e, t) {
    if (e) {
      var i = this.indexFromPos(e);
      this.cm6.dispatch({
        effects: _.scrollIntoView(i)
      });
    } else
      this.cm6.dispatch({ scrollIntoView: !0, userEvent: "scroll" });
  }
  getWrapperElement() {
    return this.cm6.dom;
  }
  // for tests
  getMode() {
    return { name: this.getOption("mode") };
  }
  setSize(e, t) {
    this.cm6.dom.style.width = e + 4 + "px", this.cm6.dom.style.height = t + "px", this.refresh();
  }
  refresh() {
    this.cm6.measure();
  }
  // event listeners
  destroy() {
    this.removeOverlay();
  }
  getLastEditEnd() {
    return this.posFromIndex(this.$lastChangeEndOffset);
  }
  onChange(e) {
    this.$lineHandleChanges && this.$lineHandleChanges.push(e);
    for (let i in this.marks)
      this.marks[i].update(e.changes);
    this.virtualSelection && (this.virtualSelection.ranges = this.virtualSelection.ranges.map((i) => i.map(e.changes)));
    var t = this.curOp = this.curOp || {};
    e.changes.iterChanges((i, r, s, o, l) => {
      (t.$changeStart == null || t.$changeStart > s) && (t.$changeStart = s), this.$lastChangeEndOffset = o;
      var a = { text: l.toJSON() };
      t.lastChange ? t.lastChange.next = t.lastChange = a : t.lastChange = t.change = a;
    }, !0), t.changeHandlers || (t.changeHandlers = this._handlers.change && this._handlers.change.slice());
  }
  onSelectionChange() {
    var e = this.curOp = this.curOp || {};
    e.cursorActivityHandlers || (e.cursorActivityHandlers = this._handlers.cursorActivity && this._handlers.cursorActivity.slice()), this.curOp.cursorActivity = !0;
  }
  operation(e, t) {
    this.curOp || (this.curOp = { $d: 0 }), this.curOp.$d++;
    try {
      var i = e();
    } finally {
      this.curOp && (this.curOp.$d--, this.curOp.$d || this.onBeforeEndOperation());
    }
    return i;
  }
  onBeforeEndOperation() {
    var e = this.curOp, t = !1;
    e && (e.change && yf(e.changeHandlers, this, e.change), e && e.cursorActivity && (yf(e.cursorActivityHandlers, this, null), e.isVimOp && (t = !0)), this.curOp = null), t && this.scrollIntoView();
  }
  moveH(e, t) {
    if (t == "char") {
      var i = this.getCursor();
      this.setCursor(i.line, i.ch + e);
    }
  }
  setOption(e, t) {
    switch (e) {
      case "keyMap":
        this.state.keyMap = t;
        break;
      case "textwidth":
        this.state.textwidth = t;
        break;
    }
  }
  getOption(e) {
    switch (e) {
      case "firstLineNumber":
        return 1;
      case "tabSize":
        return this.cm6.state.tabSize || 4;
      case "readOnly":
        return this.cm6.state.readOnly;
      case "indentWithTabs":
        return this.cm6.state.facet(ir) == "	";
      // TODO
      case "indentUnit":
        return this.cm6.state.facet(ir).length || 2;
      case "textwidth":
        return this.state.textwidth;
      // for tests
      case "keyMap":
        return this.state.keyMap || "vim";
    }
  }
  toggleOverwrite(e) {
    this.state.overwrite = e;
  }
  getTokenTypeAt(e) {
    var t, i = this.indexFromPos(e), r = ed(this.cm6.state, i), s = r?.resolve(i), o = ((t = s?.type) === null || t === void 0 ? void 0 : t.name) || "";
    return /comment/i.test(o) ? "comment" : /string/i.test(o) ? "string" : "";
  }
  overWriteSelection(e) {
    var t = this.cm6.state.doc, i = this.cm6.state.selection, r = i.ranges.map((s) => {
      if (s.empty) {
        var o = s.to < t.length ? t.sliceString(s.from, s.to + 1) : "";
        if (o && !/\n/.test(o))
          return E.range(s.from, s.to + 1);
      }
      return s;
    });
    this.cm6.dispatch({
      selection: E.create(r, i.mainIndex)
    }), this.replaceSelection(e);
  }
  /*** multiselect ****/
  isInMultiSelectMode() {
    return this.cm6.state.selection.ranges.length > 1;
  }
  virtualSelectionMode() {
    return !!this.virtualSelection;
  }
  forEachSelection(e) {
    var t = this.cm6.state.selection;
    this.virtualSelection = E.create(t.ranges, t.mainIndex);
    for (var i = 0; i < this.virtualSelection.ranges.length; i++) {
      var r = this.virtualSelection.ranges[i];
      r && (this.cm6.dispatch({ selection: E.create([r]) }), e(), this.virtualSelection.ranges[i] = this.cm6.state.selection.ranges[0]);
    }
    this.cm6.dispatch({ selection: this.virtualSelection }), this.virtualSelection = null;
  }
  hardWrap(e) {
    return Jk(this, e);
  }
}
se.isMac = typeof navigator < "u" && /* @__PURE__ */ /Mac/.test(navigator.platform);
se.Pos = Ut;
se.StringStream = Ky;
se.commands = {
  cursorCharLeft: function(n) {
    ma(n.cm6);
  },
  redo: function(n) {
    bf(n, !1);
  },
  undo: function(n) {
    bf(n, !0);
  },
  newlineAndIndent: function(n) {
    Rl({
      state: n.cm6.state,
      dispatch: (e) => zn(n, e)
    });
  },
  indentAuto: function(n) {
    jd(n.cm6);
  },
  newlineAndIndentContinueComment: void 0,
  save: void 0
};
se.isWordChar = function(n) {
  return Fl.test(n);
};
se.keys = qk;
se.addClass = function(n, e) {
};
se.rmClass = function(n, e) {
};
se.e_preventDefault = function(n) {
  n.preventDefault();
};
se.e_stop = function(n) {
  var e, t;
  (e = n?.stopPropagation) === null || e === void 0 || e.call(n), (t = n?.preventDefault) === null || t === void 0 || t.call(n);
};
se.lookupKey = function(e, t, i) {
  var r = se.keys[e];
  !r && /^Arrow/.test(e) && (r = se.keys[e.slice(5)]), r && i(r);
};
se.on = Cp;
se.off = Op;
se.signal = Ap;
se.findMatchingTag = Yk;
se.findEnclosingTag = Gk;
se.keyName = void 0;
function Mp(n, e, t) {
  var i = document.createElement("div");
  return i.appendChild(e), i;
}
function Tp(n, e) {
  n.state.currentNotificationClose && n.state.currentNotificationClose(), n.state.currentNotificationClose = e;
}
function Kk(n, e, t) {
  Tp(n, l);
  var i = Mp(n, e, t && t.bottom), r = !1, s, o = t && typeof t.duration < "u" ? t.duration : 5e3;
  function l() {
    r || (r = !0, clearTimeout(s), i.remove(), Rp(n, i));
  }
  return i.onclick = function(a) {
    a.preventDefault(), l();
  }, Pp(n, i), o && (s = setTimeout(l, o)), l;
}
function Pp(n, e) {
  var t = n.state.dialog;
  n.state.dialog = e, e.style.flex = "1", e && t !== e && (t && t.contains(document.activeElement) && n.focus(), t && t.parentElement ? t.parentElement.replaceChild(e, t) : t && t.remove(), se.signal(n, "dialog"));
}
function Rp(n, e) {
  n.state.dialog == e && (n.state.dialog = null, se.signal(n, "dialog"));
}
function _k(n, e, t, i) {
  i || (i = {}), Tp(n, void 0);
  var r = Mp(n, e, i.bottom), s = !1;
  Pp(n, r);
  function o(a) {
    if (typeof a == "string")
      l.value = a;
    else {
      if (s)
        return;
      s = !0, Rp(n, r), n.state.dialog || n.focus(), i.onClose && i.onClose(r);
    }
  }
  var l = r.getElementsByTagName("input")[0];
  return l && (i.value && (l.value = i.value, i.selectValueOnOpen !== !1 && l.select()), i.onInput && se.on(l, "input", function(a) {
    i.onInput(a, l.value, o);
  }), i.onKeyUp && se.on(l, "keyup", function(a) {
    i.onKeyUp(a, l.value, o);
  }), se.on(l, "keydown", function(a) {
    i && i.onKeyDown && i.onKeyDown(a, l.value, o) || (a.keyCode == 13 && t && t(l.value), (a.keyCode == 27 || i.closeOnEnter !== !1 && a.keyCode == 13) && (l.blur(), se.e_stop(a), o()));
  }), i.closeOnBlur !== !1 && se.on(l, "blur", function() {
    setTimeout(function() {
      document.activeElement !== l && o();
    });
  }), l.focus()), o;
}
var jk = { "(": ")>", ")": "(<", "[": "]>", "]": "[<", "{": "}>", "}": "{<", "<": ">>", ">": "<<" };
function Uk(n) {
  return n && n.bracketRegex || /[(){}[\]]/;
}
function Xk(n, e, t, i, r) {
  for (var s = r && r.maxScanLineLength || 1e4, o = r && r.maxScanLines || 1e3, l = [], a = Uk(r), f = t > 0 ? Math.min(e.line + o, n.lastLine() + 1) : Math.max(n.firstLine() - 1, e.line - o), u = e.line; u != f; u += t) {
    var g = n.getLine(u);
    if (g) {
      var y = t > 0 ? 0 : g.length - 1, b = t > 0 ? g.length : -1;
      if (!(g.length > s))
        for (u == e.line && (y = e.ch - (t < 0 ? 1 : 0)); y != b; y += t) {
          var k = g.charAt(y);
          if (a.test(k)) {
            var C = jk[k];
            if (C && C.charAt(1) == ">" == t > 0)
              l.push(k);
            else if (l.length)
              l.pop();
            else
              return { pos: new Ut(u, y), ch: k };
          }
        }
    }
  }
  return u - t == (t > 0 ? n.lastLine() : n.firstLine()) ? !1 : null;
}
function Yk(n, e) {
  return null;
}
function Gk(n, e) {
  var t, i, r = n.cm6.state, s = n.indexFromPos(e);
  if (s < r.doc.length) {
    var o = r.sliceDoc(s, s + 1);
    o == "<" && s++;
  }
  for (var l = ed(r, s), a = l?.resolve(s) || null; a; ) {
    if (((t = a.firstChild) === null || t === void 0 ? void 0 : t.type.name) == "OpenTag" && ((i = a.lastChild) === null || i === void 0 ? void 0 : i.type.name) == "CloseTag")
      return {
        open: xf(r.doc, a.firstChild),
        close: xf(r.doc, a.lastChild)
      };
    a = a.parent;
  }
}
function xf(n, e) {
  return {
    from: St(n, e.from),
    to: St(n, e.to)
  };
}
class Zk {
  constructor(e, t, i) {
    this.cm = e, this.id = e.$mid++, this.offset = t, this.assoc = i, e.marks[this.id] = this;
  }
  clear() {
    delete this.cm.marks[this.id];
  }
  find() {
    return this.offset == null ? null : this.cm.posFromIndex(this.offset);
  }
  update(e) {
    this.offset != null && (this.offset = e.mapPos(this.offset, this.assoc, Xe.TrackDel));
  }
}
function Jk(n, e) {
  for (var t, i = e.column || n.getOption("textwidth") || 80, r = e.allowMerge != !1, s = Math.min(e.from, e.to), o = Math.max(e.from, e.to); s <= o; ) {
    var l = n.getLine(s);
    if (l.length > i) {
      var a = k(l, i, 5);
      if (a) {
        var f = (t = /^\s*/.exec(l)) === null || t === void 0 ? void 0 : t[0];
        n.replaceRange(`
` + f, new Ut(s, a.start), new Ut(s, a.end));
      }
      o++;
    } else if (r && /\S/.test(l) && s != o) {
      var u = n.getLine(s + 1);
      if (u && /\S/.test(u)) {
        var g = l.replace(/\s+$/, ""), y = u.replace(/^\s+/, ""), b = g + " " + y, a = k(b, i, 5);
        a && a.start > g.length || b.length < i ? (n.replaceRange(" ", new Ut(s, g.length), new Ut(s + 1, u.length - y.length)), s--, o--) : g.length < l.length && n.replaceRange("", new Ut(s, g.length), new Ut(s, l.length));
      }
    }
    s++;
  }
  return s;
  function k(C, M, R) {
    if (!(C.length < M)) {
      var I = C.slice(0, M), N = C.slice(M), z = /^(?:(\s+)|(\S+)(\s+))/.exec(N), F = /(?:(\s+)|(\s+)(\S+))$/.exec(I), H = 0, Q = 0;
      if (F && !F[2] && (H = M - F[1].length, Q = M), z && !z[2] && (H || (H = M), Q = M + z[1].length), H)
        return {
          start: H,
          end: Q
        };
      if (F && F[2] && F.index > R)
        return {
          start: F.index,
          end: F.index + F[2].length
        };
      if (z && z[2])
        return H = M + z[2].length, {
          start: H,
          end: H + z[3].length
        };
    }
  }
}
let Wl = H0 || /* @__PURE__ */ (function() {
  let n = { cursorBlinkRate: 1200 };
  return function() {
    return n;
  };
})();
class ew {
  constructor(e, t, i, r, s, o, l, a, f, u) {
    this.left = e, this.top = t, this.height = i, this.fontFamily = r, this.fontSize = s, this.fontWeight = o, this.color = l, this.className = a, this.letter = f, this.partial = u;
  }
  draw() {
    let e = document.createElement("div");
    return e.className = this.className, this.adjust(e), e;
  }
  adjust(e) {
    e.style.left = this.left + "px", e.style.top = this.top + "px", e.style.height = this.height + "px", e.style.lineHeight = this.height + "px", e.style.fontFamily = this.fontFamily, e.style.fontSize = this.fontSize, e.style.fontWeight = this.fontWeight, e.style.color = this.partial ? "transparent" : this.color, e.className = this.className, e.textContent = this.letter;
  }
  eq(e) {
    return this.left == e.left && this.top == e.top && this.height == e.height && this.fontFamily == e.fontFamily && this.fontSize == e.fontSize && this.fontWeight == e.fontWeight && this.color == e.color && this.className == e.className && this.letter == e.letter;
  }
}
class tw {
  constructor(e, t) {
    this.view = e, this.rangePieces = [], this.cursors = [], this.cm = t, this.measureReq = { read: this.readPos.bind(this), write: this.drawSel.bind(this) }, this.cursorLayer = e.scrollDOM.appendChild(document.createElement("div")), this.cursorLayer.className = "cm-cursorLayer cm-vimCursorLayer", this.cursorLayer.setAttribute("aria-hidden", "true"), e.requestMeasure(this.measureReq), this.setBlinkRate();
  }
  setBlinkRate() {
    let t = Wl(this.cm.cm6.state).cursorBlinkRate;
    this.cursorLayer.style.animationDuration = t + "ms";
  }
  update(e) {
    (e.selectionSet || e.geometryChanged || e.viewportChanged) && (this.view.requestMeasure(this.measureReq), this.cursorLayer.style.animationName = this.cursorLayer.style.animationName == "cm-blink" ? "cm-blink2" : "cm-blink"), iw(e) && this.setBlinkRate();
  }
  scheduleRedraw() {
    this.view.requestMeasure(this.measureReq);
  }
  readPos() {
    let { state: e } = this.view, t = [];
    for (let i of e.selection.ranges) {
      let r = i == e.selection.main, s = ow(this.cm, this.view, i, r);
      s && t.push(s);
    }
    return { cursors: t };
  }
  drawSel({ cursors: e }) {
    if (e.length != this.cursors.length || e.some((t, i) => !t.eq(this.cursors[i]))) {
      let t = this.cursorLayer.children;
      if (t.length !== e.length) {
        this.cursorLayer.textContent = "";
        for (const i of e)
          this.cursorLayer.appendChild(i.draw());
      } else
        e.forEach((i, r) => i.adjust(t[r]));
      this.cursors = e;
    }
  }
  destroy() {
    this.cursorLayer.remove();
  }
}
function iw(n) {
  return Wl(n.startState) != Wl(n.state);
}
const nw = {
  ".cm-vimMode .cm-line": {
    "& ::selection": { backgroundColor: "transparent !important" },
    "&::selection": { backgroundColor: "transparent !important" },
    caretColor: "transparent !important"
  },
  ".cm-fat-cursor": {
    position: "absolute",
    background: "#ff9696",
    border: "none",
    whiteSpace: "pre"
  },
  "&:not(.cm-focused) .cm-fat-cursor": {
    background: "none",
    outline: "solid 1px #ff9696",
    color: "transparent !important"
  }
}, rw = /* @__PURE__ */ Ti.highest(/* @__PURE__ */ _.theme(nw));
function sw(n) {
  let e = n.scrollDOM.getBoundingClientRect();
  return { left: (n.textDirection == xe.LTR ? e.left : e.right - n.scrollDOM.clientWidth) - n.scrollDOM.scrollLeft * n.scaleX, top: e.top - n.scrollDOM.scrollTop * n.scaleY };
}
function ow(n, e, t, i) {
  var r, s, o, l;
  let a = t.head, f = !1, u = 1, g = n.state.vim;
  if (g && (!g.insertMode || n.state.overwrite)) {
    if (f = !0, g.visualBlock && !i)
      return null;
    t.anchor < t.head && (a < e.state.doc.length && e.state.sliceDoc(a, a + 1)) != `
` && a--, n.state.overwrite ? u = 0.2 : g.status && (u = 0.5);
  }
  if (f) {
    let b = a < e.state.doc.length && e.state.sliceDoc(a, a + 1);
    b && /[\uDC00-\uDFFF]/.test(b) && a > 1 && (a--, b = e.state.sliceDoc(a, a + 1));
    let k = e.coordsAtPos(a, 1);
    if (!k)
      return null;
    let C = sw(e), M = e.domAtPos(a), R = M ? M.node : e.contentDOM;
    for (R instanceof Text && M.offset >= R.data.length && !((r = R.parentElement) === null || r === void 0) && r.nextSibling && (R = (s = R.parentElement) === null || s === void 0 ? void 0 : s.nextSibling, M = { node: R, offset: 0 }); M && M.node instanceof HTMLElement; )
      R = M.node, M = { node: M.node.childNodes[M.offset], offset: 0 };
    if (!(R instanceof HTMLElement)) {
      if (!R.parentNode)
        return null;
      R = R.parentNode;
    }
    let I = getComputedStyle(R), N = k.left, z = (l = (o = e).coordsForChar) === null || l === void 0 ? void 0 : l.call(o, a);
    if (z && (N = z.left), !b || b == `
` || b == "\r")
      b = " ";
    else if (b == "	") {
      b = " ";
      var y = e.coordsAtPos(a + 1, -1);
      y && (N = y.left - (y.left - k.left) / parseInt(I.tabSize));
    } else /[\uD800-\uDBFF]/.test(b) && a < e.state.doc.length - 1 && (b += e.state.sliceDoc(a + 1, a + 2));
    let F = k.bottom - k.top;
    return new ew((N - C.left) / e.scaleX, (k.top - C.top + F * (1 - u)) / e.scaleY, F * u / e.scaleY, I.fontFamily, I.fontSize, I.fontWeight, I.color, i ? "cm-fat-cursor cm-cursor-primary" : "cm-fat-cursor cm-cursor-secondary", b, u != 1);
  } else
    return null;
}
var lw = typeof navigator < "u" && /* @__PURE__ */ /linux/i.test(navigator.platform) && /* @__PURE__ */ / Gecko\/\d+/.exec(navigator.userAgent);
const Ei = /* @__PURE__ */ $k(se), aw = 250, hw = /* @__PURE__ */ _.baseTheme({
  ".cm-vimMode .cm-cursorLayer:not(.cm-vimCursorLayer)": {
    display: "none"
  },
  ".cm-vim-panel": {
    padding: "0px 10px",
    fontFamily: "monospace",
    minHeight: "1.3em",
    display: "flex"
  },
  ".cm-vim-panel input": {
    border: "none",
    outline: "none",
    backgroundColor: "inherit"
  },
  "&light .cm-searchMatch": { backgroundColor: "#ffff0054" },
  "&dark .cm-searchMatch": { backgroundColor: "#00ffff8a" }
}), cw = /* @__PURE__ */ De.fromClass(class {
  constructor(n) {
    this.status = "", this.query = null, this.decorations = G.none, this.waitForCopy = !1, this.lastKeydown = "", this.useNextTextInput = !1, this.compositionText = "", this.view = n;
    const e = this.cm = new se(n);
    Ei.enterVimMode(this.cm), this.view.cm = this.cm, this.cm.state.vimPlugin = this, this.blockCursor = new tw(n, e), this.updateClass(), this.cm.on("vim-command-done", () => {
      e.state.vim && (e.state.vim.status = ""), this.blockCursor.scheduleRedraw(), this.updateStatus();
    }), this.cm.on("vim-mode-change", (t) => {
      e.state.vim && (e.state.vim.mode = t.mode, t.subMode && (e.state.vim.mode += " block"), e.state.vim.status = "", this.blockCursor.scheduleRedraw(), this.updateClass(), this.updateStatus());
    }), this.cm.on("dialog", () => {
      this.cm.state.statusbar ? this.updateStatus() : n.dispatch({
        effects: Lp.of(!!this.cm.state.dialog)
      });
    }), this.dom = document.createElement("span"), this.spacer = document.createElement("span"), this.spacer.style.flex = "1", this.statusButton = document.createElement("span"), this.statusButton.onclick = (t) => {
      Ei.handleKey(this.cm, "<Esc>", "user"), this.cm.focus();
    }, this.statusButton.style.cssText = "cursor: pointer";
  }
  update(n) {
    var e;
    if ((n.viewportChanged || n.docChanged) && this.query && this.highlight(this.query), n.docChanged && this.cm.onChange(n), n.selectionSet && this.cm.onSelectionChange(), n.viewportChanged, this.cm.curOp && !this.cm.curOp.isVimOp && this.cm.onBeforeEndOperation(), n.transactions) {
      for (let t of n.transactions)
        for (let i of t.effects)
          if (i.is(Mi))
            if (!((e = i.value) === null || e === void 0 ? void 0 : e.forVim))
              this.highlight(null);
            else {
              let s = i.value.create();
              this.highlight(s);
            }
    }
    this.blockCursor.update(n);
  }
  updateClass() {
    const n = this.cm.state;
    !n.vim || n.vim.insertMode && !n.overwrite ? this.view.scrollDOM.classList.remove("cm-vimMode") : this.view.scrollDOM.classList.add("cm-vimMode");
  }
  updateStatus() {
    let n = this.cm.state.statusbar, e = this.cm.state.vim;
    if (!n || !e)
      return;
    let t = this.cm.state.dialog;
    if (t)
      t.parentElement != n && (n.textContent = "", n.appendChild(t));
    else {
      n.textContent = "";
      var i = (e.mode || "normal").toUpperCase();
      e.insertModeReturn && (i += "(C-O)"), this.statusButton.textContent = `--${i}--`, n.appendChild(this.statusButton), n.appendChild(this.spacer);
    }
    this.dom.textContent = e.status, n.appendChild(this.dom);
  }
  destroy() {
    Ei.leaveVimMode(this.cm), this.updateClass(), this.blockCursor.destroy(), delete this.view.cm;
  }
  highlight(n) {
    if (this.query = n, !n)
      return this.decorations = G.none;
    let { view: e } = this, t = new ei();
    for (let i = 0, r = e.visibleRanges, s = r.length; i < s; i++) {
      let { from: o, to: l } = r[i];
      for (; i < s - 1 && l > r[i + 1].from - 2 * aw; )
        l = r[++i].to;
      n.highlight(e.state, o, l, (a, f) => {
        t.add(a, f, uw);
      });
    }
    return this.decorations = t.finish();
  }
  handleKey(n, e) {
    const t = this.cm;
    let i = t.state.vim;
    if (!i)
      return;
    const r = Ei.vimKeyFromEvent(n, i);
    if (se.signal(this.cm, "inputEvent", { type: "handleKey", key: r }), !r)
      return;
    if (r == "<Esc>" && !i.insertMode && !i.visualMode && this.query) {
      const l = i.searchState_;
      l && (t.removeOverlay(l.getOverlay()), l.setOverlay(null));
    }
    if (r === "<C-c>" && !se.isMac && t.somethingSelected())
      return this.waitForCopy = !0, !0;
    i.status = (i.status || "") + r;
    let o = Ei.multiSelectHandleKey(t, r, "user");
    return i = Ei.maybeInitVimState_(t), !o && i.insertMode && t.state.overwrite && (n.key && n.key.length == 1 && !/\n/.test(n.key) ? (o = !0, t.overWriteSelection(n.key)) : n.key == "Backspace" && (o = !0, se.commands.cursorCharLeft(t))), o && (se.signal(this.cm, "vim-keypress", r), n.preventDefault(), n.stopPropagation(), this.blockCursor.scheduleRedraw()), this.updateStatus(), !!o;
  }
}, {
  eventHandlers: {
    copy: function(n, e) {
      this.waitForCopy && (this.waitForCopy = !1, Promise.resolve().then(() => {
        var t = this.cm, i = t.state.vim;
        i && (i.insertMode ? t.setSelection(t.getCursor(), t.getCursor()) : t.operation(() => {
          t.curOp && (t.curOp.isVimOp = !0), Ei.handleKey(t, "<Esc>", "user");
        }));
      }));
    },
    compositionstart: function(n, e) {
      this.useNextTextInput = !0, se.signal(this.cm, "inputEvent", n);
    },
    compositionupdate: function(n, e) {
      se.signal(this.cm, "inputEvent", n);
    },
    compositionend: function(n, e) {
      se.signal(this.cm, "inputEvent", n);
    },
    keypress: function(n, e) {
      se.signal(this.cm, "inputEvent", n), this.lastKeydown == "Dead" && this.handleKey(n, e);
    },
    keydown: function(n, e) {
      se.signal(this.cm, "inputEvent", n), this.lastKeydown = n.key, this.lastKeydown == "Unidentified" || this.lastKeydown == "Process" || this.lastKeydown == "Dead" ? this.useNextTextInput = !0 : (this.useNextTextInput = !1, this.handleKey(n, e));
    }
  },
  provide: () => [
    _.inputHandler.of((n, e, t, i) => {
      var r, s, o = mw(n);
      if (!o)
        return !1;
      var l = (r = o.state) === null || r === void 0 ? void 0 : r.vim, a = o.state.vimPlugin;
      if (l && !l.insertMode && !(!((s = o.curOp) === null || s === void 0) && s.isVimOp)) {
        if (i === "\0\0")
          return !0;
        if (se.signal(o, "inputEvent", {
          type: "text",
          text: i,
          from: e,
          to: t
        }), i.length == 1 && a.useNextTextInput) {
          if (l.expectLiteralNext && n.composing)
            return a.compositionText = i, !1;
          if (a.compositionText) {
            var f = a.compositionText;
            a.compositionText = "";
            var u = n.state.selection.main.head, g = n.state.sliceDoc(u - f.length, u);
            if (f === g) {
              var y = o.getCursor();
              o.replaceRange("", o.posFromIndex(u - f.length), y);
            }
          }
          return a.handleKey({
            key: i,
            preventDefault: () => {
            },
            stopPropagation: () => {
            }
          }), fw(n), !0;
        }
      }
      return !1;
    })
  ],
  decorations: (n) => n.decorations
});
function fw(n) {
  var e = n.scrollDOM.parentElement;
  if (e) {
    if (lw) {
      n.contentDOM.textContent = "\0\0", n.contentDOM.dispatchEvent(new CustomEvent("compositionend"));
      return;
    }
    var t = n.scrollDOM.nextSibling, i = window.getSelection(), r = i && {
      anchorNode: i.anchorNode,
      anchorOffset: i.anchorOffset,
      focusNode: i.focusNode,
      focusOffset: i.focusOffset
    };
    n.scrollDOM.remove(), e.insertBefore(n.scrollDOM, t);
    try {
      r && i && (i.setPosition(r.anchorNode, r.anchorOffset), r.focusNode && i.extend(r.focusNode, r.focusOffset));
    } catch (s) {
      console.error(s);
    }
    n.focus(), n.contentDOM.dispatchEvent(new CustomEvent("compositionend"));
  }
}
const uw = /* @__PURE__ */ G.mark({ class: "cm-searchMatch" }), Lp = /* @__PURE__ */ ne.define(), dw = /* @__PURE__ */ $e.define({
  create: () => !1,
  update(n, e) {
    for (let t of e.effects)
      t.is(Lp) && (n = t.value);
    return n;
  },
  provide: (n) => ji.from(n, (e) => e ? pw : null)
});
function pw(n) {
  let e = document.createElement("div");
  e.className = "cm-vim-panel";
  let t = n.cm;
  return t.state.dialog && e.appendChild(t.state.dialog), { top: !1, dom: e };
}
function gw(n) {
  let e = document.createElement("div");
  e.className = "cm-vim-panel";
  let t = n.cm;
  return t.state.statusbar = e, t.state.vimPlugin.updateStatus(), { dom: e };
}
function Iw(n = {}) {
  return [
    hw,
    cw,
    rw,
    n.status ? ji.of(gw) : dw
  ];
}
function mw(n) {
  return n.cm || null;
}
const kf = 39;
function vw(n) {
  return n >= 65 && n <= 90;
}
function yw(n) {
  return n >= 65 && n <= 90 || n >= 48 && n <= 57 || n === 95;
}
function bw(n) {
  return n >= 97 && n <= 122 || n >= 48 && n <= 57 || n === 95 || n === 46 || n === 45;
}
const xw = new vp(
  (n, e) => {
    if (n.next !== 60 || (n.advance(), n.next !== 60) || (n.advance(), !vw(n.next))) return;
    let t = "";
    for (; yw(n.next); )
      t += String.fromCharCode(n.next), n.advance();
    if (n.next === 44 && (n.advance(), n.next >= 97 && n.next <= 122))
      for (; bw(n.next); )
        n.advance();
    if (!(n.next !== 10 && n.next !== 13)) {
      for (n.next === 13 && n.advance(), n.next === 10 && n.advance(); n.next !== -1; ) {
        for (; n.next === 32 || n.next === 9; )
          n.advance();
        let i = 0, r = !0;
        n.pos;
        for (let s = 0; s < t.length && n.next !== -1; s++) {
          if (n.next !== t.charCodeAt(s)) {
            r = !1;
            break;
          }
          n.advance(), i++;
        }
        if (r && i === t.length && (n.next === 10 || n.next === 13 || n.next === -1)) {
          n.acceptToken(kf);
          return;
        }
        for (; n.next !== 10 && n.next !== 13 && n.next !== -1; )
          n.advance();
        n.next === 13 && n.advance(), n.next === 10 && n.advance();
      }
      n.acceptToken(kf);
    }
  },
  { contextual: !1 }
), kw = Qs({
  // Keys (property names) - first atom in an Entry
  "KeyAtom/BareScalar": P.propertyName,
  "KeyAtom/QuotedScalar": P.propertyName,
  "KeyPayload/BareScalar": P.propertyName,
  "KeyPayload/QuotedScalar": P.propertyName,
  // Values - second atom in an Entry
  "ValueAtom/BareScalar": P.string,
  "ValueAtom/QuotedScalar": P.string,
  "ValuePayload/BareScalar": P.string,
  "ValuePayload/QuotedScalar": P.string,
  // Sequence items
  "SeqAtom/BareScalar": P.string,
  "SeqAtom/QuotedScalar": P.string,
  "SeqPayload/BareScalar": P.string,
  "SeqPayload/QuotedScalar": P.string,
  // Tags (@foo)
  Tag: P.tagName,
  // Raw strings and heredocs
  RawScalar: P.special(P.string),
  Heredoc: P.special(P.string),
  // Inline attributes (key>value pairs)
  Attributes: P.special(P.variableName),
  Unit: P.null,
  Comment: P.lineComment,
  DocComment: P.docComment,
  "( )": P.paren,
  "{ }": P.brace,
  ",": P.separator
}), Dp = An.deserialize({
  version: 14,
  states: "*tQVQPOOOOQO'#C^'#C^OzQPO'#C_OOQQ'#Cb'#CbOOQQ'#Cd'#CdOOQQ'#Ce'#CeO#aQPO'#CnOOQQ'#Cs'#CsOOQQ'#Ct'#CtOOQQ'#Cv'#CvO$VQPO'#ChOOQQ'#Cw'#CwO$rQRO'#CaOOQQ'#Ca'#CaO%lQRO'#C`O&SQPO'#DWOOQO'#DW'#DWOOQO'#C|'#C|QVQPOOOOQO'#C}'#C}OOQO-E6{-E6{OOQO'#Cp'#CpOOQO'#DP'#DPO!lQPO'#CoOOQO'#Cr'#CrO&XQPO'#CrO&cQPO'#CoOOQQ,59Y,59YO&nQPO,59YOOQO'#DO'#DOO#hQPO'#CiOOQO'#Cu'#CuO&sQPO'#CjOOQO'#Cj'#CjO'gQPO'#CiOOQQ,59S,59SO'nQPO,59SOOQQ'#Cc'#CcOOQQ,58{,58{OOQO'#C{'#C{OOQO'#Cz'#CzO'sQPO'#CxOOQO'#Cx'#CxOOQO,58z,58zOOQO,59r,59rOOQO-E6z-E6zOOQO-E6}-E6}O(QQPO,59ZOOQO,59^,59^O(]QPO'#DPO(QQPO,59ZO(QQPO,59ZOOQQ1G.t1G.tOOQO-E6|-E6|O(sQPO,59TOOQO'#Ck'#CkOOQO,59U,59UO(sQPO,59TOOQO'#DR'#DRO(sQPO,59TOOQQ1G.n1G.nOOQO'#Cy'#CyOOQO,59d,59dO(zQPO1G.uO(zQPO1G.uOOQO,59l,59lOOQO-E7O-E7OO)VQPO1G.oO)VQPO1G.oOOQO,59m,59mOOQO-E7P-E7PO)^QPO7+$aP)iQPO'#DQO)vQPO7+$ZP#hQPO'#DO",
  stateData: "*W~OyOS~OZYOaUO{PO|`O}cO!ORO!PSO!QTO!RVO!SWO!TXO~O}cO|RXZRXaRX!ORX!PRX!QRX!RRX!SRX!TRX~OZYOaUOeeO|eO}cO!ORO!PSO!QTO!RVO!SWO!TXO~O`kO~P!lOZYOaUO|mO!ORO!PSO!QTO!RVO!SWO!TXO~OYsO~P#hOZYOaUO!PSO!QTO!RVO!SWO~OZTXaTXwTX|TX!OTX!PTX!QTX!RTX!STX!TTX`TXeTX~P$^OwwO!ORO!TXO|SX`SXeSX~P$^O||O~O!ORO!TXO~P$^OeeO|eO`cX~O`!UO~OY^XZ^Xa^X|^X!O^X!P^X!Q^X!R^X!S^X!T^X~P$^OY]X~P#hOY!^O~O|lX`lXelX~P$^OeeO|eO`ca~O}cO!ORO!TXO`sXesX|sX~P$^OY]a~P#hOeeO|eO`ci~OY]i~P#hOeeO|eO`cq~O}cO!ORO!TXO~P$^OY]q~P#hO!O!R}{!Q!S!T{~",
  goto: "(Q{PP|!Q![!i!r#[#_#_PP#_$Q$T$gPP#_$j$mP$z#_#_%U%b%z&T&W&Z%w&^&d&o'T'h'rPPPP'|T_ObS_ObXiUg!R!jS_ObWhUg!R!jR!Qi_^OUbgi!R!j^[OUbgi!R!jdpYnr!W!Z!]!e!f!k!lRy^Rv[^ZOUbgi!R!jdoYnr!W!Z!]!e!f!k!lQu[Qx^Q!XpR!_yRtYQrYQ!WnW![r!W!]!fX!g!Z!e!k!lR!YpRlUYfUg!S!a!iX!Rj!P!T!bQjUQ!PgT!c!R!jeqYnr!W!Z!]!e!f!k!l^ZOUbgi!R!jdoYnr!W!Z!]!e!f!k!lRx^_]OUbgi!R!jR{^R!`yRz^QbOR}b[QOUbg!R!jRdQQnYY!Vn!Z!e!k!lQ!ZrS!e!W!]R!k!fQgUW!Og!S!a!iQ!SjS!a!P!TR!i!bQ!TjQ!b!PT!d!T!bQ!]rQ!f!WT!h!]!fTaOb",
  nodeNames: "⚠ Document Comment DocComment Entry KeyExpr Tag KeyPayload QuotedScalar RawScalar ) ( Sequence SeqContent SeqItem SeqPayload } { Object ObjContent ObjSep , ObjItem Unit Attributes SeqAtom BareScalar KeyAtom ValueExpr ValuePayload ValueAtom Heredoc",
  maxTerm: 51,
  nodeProps: [
    ["openedBy", 10, "(", 16, "{"],
    ["closedBy", 11, ")", 17, "}"]
  ],
  propSources: [kw],
  skippedNodes: [0],
  repeatNodeCount: 6,
  tokenData: "B[~RmOX!|XY(UYZ(aZ]!|]^(f^p!|pq(Uqr!|rs(lsx!|xy)syz)xz|!||})}}!P!|!P!Q*S!Q!^!|!^!_$Q!a!b!|!b!c?b!c#f!|#f#g@U#g#o!|#o#pBQ#p#q!|#q#rBV#r;'S!|;'S;=`(O<%lO!|~#R_!T~OX!|Z]!|^p!|qr!|sx!|z|!|}!^!|!^!_$Q!_!`!|!`!a$|!a#o!|#p#q!|#r;'S!|;'S;=`(O<%lO!|~$T]OX$QZ]$Q^p$Qqr$Qsx$Qz|$Q}!`$Q!`!a$|!a#o$Q#p#q$Q#r;'S$Q;'S;=`'r<%lO$Q~%PZOX%rZ]%r^p%rqr%rsx%rz|%r}#o%r#p#q%r#r;'S%r;'S;=`'x<%lO%r~%w]!S~OX%rXY&pZ]%r^p%rpq&pqr%rsx%rz|%r}#o%r#p#q%r#r;'S%r;'S;=`'x<%lO%r~&s_OX$QXY&pZ]$Q^p$Qpq&pqr$Qsx$Qz|$Q}!_$Q!a!b$Q!c#o$Q#p#q$Q#r;'S$Q;'S;=`'r<%lO$Q~'uP;=`<%l$Q~'{P;=`<%l%r~(RP;=`<%l!|~(ZQy~XY(Upq(U~(fO|~~(iPYZ(a~(oXOY(lZ](l^r(lrs)[s#O(l#O#P)a#P;'S(l;'S;=`)m<%lO(l~)aO!P~~)dRO;'S(l;'S;=`)m<%lO(l~)pP;=`<%l(l~)xOZ~~)}OY~~*SOe~~*Xa!T~OX!|Z]!|^p!|qr!|sx!|z|!|}!P!|!P!Q+^!Q!^!|!^!_$Q!_!`!|!`!a$|!a#o!|#p#q!|#r;'S!|;'S;=`(O<%lO!|~+ch!T~OX,}XY.pZ],}^p,}pq.pqr,}rs.psx,}xz.pz|,}|}.p}!P,}!P!Q5u!Q!^,}!^!_/_!_!`,}!`!a0x!a#o,}#o#p.p#p#q,}#q#r.p#r;'S,};'S;=`5o<%lO,}~-Uh{~!T~OX,}XY.pZ],}^p,}pq.pqr,}rs.psx,}xz.pz|,}|}.p}!^,}!^!_/_!_!`,}!`!a0x!a#Q,}#Q#R!|#R#o,}#o#p.p#p#q,}#q#r.p#r;'S,};'S;=`5o<%lO,}~.uU{~OY.pZ].p^#Q.p#R;'S.p;'S;=`/X<%lO.p~/[P;=`<%l.p~/df{~OX/_XY.pZ]/_^p/_pq.pqr/_rs.psx/_xz.pz|/_|}.p}!`/_!`!a0x!a#Q/_#Q#R$Q#R#o/_#o#p.p#p#q/_#q#r.p#r;'S/_;'S;=`5c<%lO/_~0}d{~OX2]XY.pZ]2]^p2]pq.pqr2]rs.psx2]xz.pz|2]|}.p}#Q2]#Q#R%r#R#o2]#o#p.p#p#q2]#q#r.p#r;'S2];'S;=`5i<%lO2]~2dd{~!S~OX2]XY3rZ]2]^p2]pq3rqr2]rs.psx2]xz.pz|2]|}.p}#Q2]#Q#R%r#R#o2]#o#p.p#p#q2]#q#r.p#r;'S2];'S;=`5i<%lO2]~3wh{~OX/_XY3rZ]/_^p/_pq3rqr/_rs.psx/_xz.pz|/_|}.p}!_/_!_!a.p!a!b/_!b!c.p!c#Q/_#Q#R$Q#R#o/_#o#p.p#p#q/_#q#r.p#r;'S/_;'S;=`5c<%lO/_~5fP;=`<%l/_~5lP;=`<%l2]~5rP;=`<%l,}~5zj!T~OX5uXY7lYZ8XZ]5u]^8^^p5upq7lqr5urs7lsx5uxz7lz|5u|}7l}!^5u!^!_8j!_!`5u!`!a:X!a#Q5u#Q#R!|#R#o5u#o#p7l#p#q5u#q#r7l#r;'S5u;'S;=`?[<%lO5u~7oWOY7lYZ8XZ]7l]^8^^#Q7l#R;'S7l;'S;=`8d<%lO7l~8^O}~~8aPYZ8X~8gP;=`<%l7l~8mhOX8jXY7lYZ8XZ]8j]^8^^p8jpq7lqr8jrs7lsx8jxz7lz|8j|}7l}!`8j!`!a:X!a#Q8j#Q#R$Q#R#o8j#o#p7l#p#q8j#q#r7l#r;'S8j;'S;=`?O<%lO8j~:[fOX;pXY7lYZ8XZ];p]^8^^p;ppq7lqr;prs7lsx;pxz7lz|;p|}7l}#Q;p#Q#R%r#R#o;p#o#p7l#p#q;p#q#r7l#r;'S;p;'S;=`?U<%lO;p~;uf!S~OX;pXY=ZYZ8XZ];p]^8^^p;ppq=Zqr;prs7lsx;pxz7lz|;p|}7l}#Q;p#Q#R%r#R#o;p#o#p7l#p#q;p#q#r7l#r;'S;p;'S;=`?U<%lO;p~=^jOX8jXY=ZYZ8XZ]8j]^8^^p8jpq=Zqr8jrs7lsx8jxz7lz|8j|}7l}!_8j!_!a7l!a!b8j!b!c7l!c#Q8j#Q#R$Q#R#o8j#o#p7l#p#q8j#q#r7l#r;'S8j;'S;=`?O<%lO8j~?RP;=`<%l8j~?XP;=`<%l;p~?_P;=`<%l5u~?gR!R~!c!}?p#R#S?p#T#o?p~?uT!O~}!O?p!Q![?p!c!}?p#R#S?p#T#o?p~@Za!T~OX!|Z]!|^p!|qr!|rsA`st@Utx!|z|!|}!^!|!^!_$Q!_!`!|!`!a$|!a#o!|#p#q!|#r;'S!|;'S;=`(O<%lO!|~AcTOrA`rsArs;'SA`;'S;=`Az<%lOA`~AwP!Q~stAr~A}P;=`<%lA`~BVOa~~B[O`~",
  tokenizers: [0, xw],
  topRules: { Document: [0, 1] },
  tokenPrec: 413
});
function ww(n) {
  const e = n.match(/^<<([A-Z][A-Z0-9_]*)(?:,([a-z][a-z0-9_.-]*))?\r?\n/);
  if (!e) return null;
  const t = e[1], i = e[2] || null, r = e[0].length, s = new RegExp(`^[ \\t]*${t}$`, "m"), o = n.slice(r).match(s);
  return !o || o.index === void 0 ? {
    delimiter: t,
    langHint: i,
    contentStart: r,
    contentEnd: n.length
  } : {
    delimiter: t,
    langHint: i,
    contentStart: r,
    contentEnd: r + o.index
  };
}
function Sw(n) {
  const e = /* @__PURE__ */ new Map();
  for (const { tag: t, language: i } of n)
    e.set(t, i.language.parser);
  return Kv((t, i) => {
    if (t.type.name !== "Heredoc") return null;
    const r = i.read(t.from, t.to), s = ww(r);
    if (!s || !s.langHint) return null;
    const o = e.get(s.langHint);
    return o ? {
      parser: o,
      overlay: [{ from: t.from + s.contentStart, to: t.from + s.contentEnd }]
    } : null;
  });
}
const Cw = nd.of((n, e, t) => {
  let r = Ve(n).resolveInner(t, -1);
  for (let s = r; s; s = s.parent)
    if (s.type.name === "Object" || s.type.name === "Sequence") {
      const o = s.firstChild, l = s.lastChild;
      if (o && l && o.to < l.from && o.from >= e)
        return { from: o.to, to: l.from };
    }
  return null;
}), Bp = [
  Hs.add({
    Object: rr({ except: /^\s*\}/ }),
    Sequence: rr({ except: /^\s*\)/ })
  }),
  zs.add({
    Object: Sl,
    Sequence: Sl
  })
], Ep = Ui.define({
  name: "styx",
  parser: Dp.configure({ props: Bp }),
  languageData: {
    commentTokens: { line: "//" },
    closeBrackets: { brackets: ["(", "{", '"'] }
  }
});
function Ow(n) {
  if (n.length === 0)
    return Ep;
  const e = Dp.configure({
    props: Bp,
    wrap: Sw(n)
  });
  return Ui.define({
    name: "styx",
    parser: e,
    languageData: {
      commentTokens: { line: "//" },
      closeBrackets: { brackets: ["(", "{", '"'] }
    }
  });
}
const Aw = [
  "@string",
  "@int",
  "@float",
  "@bool",
  "@null",
  "@object",
  "@array",
  "@optional",
  "@required",
  "@default",
  "@enum",
  "@pattern",
  "@min",
  "@max",
  "@minLength",
  "@maxLength"
].map((n) => ({ label: n, type: "keyword" })), Mw = Ep.data.of({
  autocomplete: Ca(Aw)
});
function Nw(n = {}) {
  const e = n.nestedLanguages || [], t = Ow(e), i = e.flatMap((r) => r.language.support);
  return new ha(t, [Mw, Cw, ...i]);
}
export {
  Ds as Compartment,
  pe as EditorState,
  _ as EditorView,
  Ti as Prec,
  Lw as basicSetup,
  Bw as json,
  ta as keymap,
  Dw as oneDark,
  Ew as sql,
  Nw as styx,
  Iw as vim
};
